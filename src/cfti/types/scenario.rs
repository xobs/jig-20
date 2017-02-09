extern crate ini;
extern crate daggy;

use self::ini::Ini;
use self::daggy::{Dag, Walker, NodeIndex};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::ops::Deref;
use super::test::Test;
use cfti::types::Jig;
use super::super::testset::TestSet;
use super::super::controller::{self, BroadcastMessageContents};

#[derive(Debug)]
pub enum ScenarioError {
    FileLoadError,
    MissingScenarioSection,
    TestListNotFound,
    TestNotFound(String),
    TestDependencyNotFound(String, String),
    CircularDependency(String, String),
}

#[derive(Copy, Clone, Debug)]
struct TestEdge;

#[derive(Debug)]
pub struct Scenario {
    /// id: The string that other units refer to this file as.
    id: String,

    /// name: Display name of this scenario.
    name: String,

    /// description: Paragraph describing this scenario.
    description: String,

    /// timeout: Maximum number of seconds this scenario should take.
    timeout: u32,

    /// tests: A vector containing all the tests in this scenario.  Will be resolved after all units are loaded.
    pub tests: Vec<Arc<Mutex<Test>>>,

    /// exec_start: A command to run when starting tests.
    exec_start: Option<String>,

    /// exec_stop_success: A command to run upon successful completion of this scenario.
    exec_stop_success: Option<String>,

    /// exec_stop_failure: A command to run if this scenario fails.
    exec_stop_failure: Option<String>,

    /// The controller where messages go.
    controller: Arc<Mutex<controller::Controller>>,
}

impl Scenario {
    pub fn new(ts: &TestSet,
               id: &str,
               path: &str,
               loaded_jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               loaded_tests: &HashMap<String, Arc<Mutex<Test>>>,
               controller: Arc<Mutex<controller::Controller>>) -> Option<Result<Scenario, ScenarioError>> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Some(Err(ScenarioError::FileLoadError)),
            Ok(s) => s,
        };

        let scenario_section = match ini_file.section(Some("Scenario")) {
            None => return Some(Err(ScenarioError::MissingScenarioSection)),
            Some(s) => s,
        };

        // Check to see if this scenario is compatible with this jig.
        match scenario_section.get("Jigs") {
            None => (),
            Some(s) => {
                let jig_names: Vec<String> = s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect();
                let mut found_it = false;
                for jig_name in jig_names {
                    if loaded_jigs.get(&jig_name).is_some() {
                        found_it = true;
                        break
                    }
                }
                if found_it == false {
                    ts.debug("scenario", id, format!("The scenario '{}' is not compatible with this jig", id).as_str());
                    return None;
                }
            }
        }

        let description = match scenario_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match scenario_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let timeout = match scenario_section.get("Timeout") {
            None => 2000,
            Some(s) => s.parse().unwrap(),
        };

        let exec_start = match scenario_section.get("ExecStart") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_success = match scenario_section.get("ExecStopSuccess") {
            None => match scenario_section.get("ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_failure = match scenario_section.get("ExecStopFail") {
            None => match scenario_section.get("ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                },
            Some(s) => Some(s.to_string()),
        };

        let test_names = match scenario_section.get("Tests") {
            None => return Some(Err(ScenarioError::TestListNotFound)),
            Some(s) => s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect(),
        };

        let tests = match Scenario::build_graph(ts, id, &test_names, &loaded_tests) {
            Err(e) => return Some(Err(e)),
            Ok(v) => v,
        };

        Some(Ok(Scenario {
            id: id.to_string(),
            tests: tests,
            timeout: timeout,
            name: name,
            description: description,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,
            controller: controller,
        }))
    }

    fn visit_node(seen_nodes: &mut HashMap<NodeIndex, ()>,
                  loaded_tests: &HashMap<String, Arc<Mutex<Test>>>,
                  node: &NodeIndex,
                  test_graph: &Dag<String, TestEdge>,
                  node_bucket: &HashMap<String, NodeIndex>,
                  test_order: &mut Vec<Arc<Mutex<Test>>>) {

        // If this node has been seen already, don't re-visit it.
        if seen_nodes.insert(node.clone(), ()).is_some() {
            return;
        }

        /*
        // 1. Visit all parents
        // 2. Visit ourselves
        // 3. Visit all children
        // Build the nodes into a vec
        */

        let parents = test_graph.parents(*node);
        for (edge_index, parent_index) in parents.iter(test_graph) {
            Scenario::visit_node(seen_nodes,
                                 loaded_tests,
                                 &parent_index,
                                 test_graph,
                                 node_bucket,
                                 test_order);
        }
        let test_item = loaded_tests.get(&test_graph[*node]).unwrap();
        test_order.push(test_item.clone());

        let children = test_graph.children(*node);
        for (edge_index, child_index) in children.iter(test_graph) {
            Scenario::visit_node(seen_nodes,
                                 loaded_tests,
                                 &child_index,
                                 test_graph,
                                 node_bucket,
                                 test_order);
        }
    }

    fn build_graph(ts: &TestSet,
                   id: &str,
                   test_names: &Vec<String>,
                   loaded_tests: &HashMap<String, Arc<Mutex<Test>>>)
                   -> Result<Vec<Arc<Mutex<Test>>>, ScenarioError> {

        // Resolve the test names.
        let mut test_graph = Dag::<String, TestEdge>::new();
        let mut node_bucket = HashMap::new();

        // Create a node for each available test.  We will add
        // edges later on as we traverse the dependency lists.
        for (test_name, _) in loaded_tests {
            node_bucket.insert(test_name.clone(),
                               test_graph.add_node(test_name.clone()));
        }

        let mut to_resolve = test_names.clone();
        let mut resolved = HashMap::new();
        loop {
            // Resolve every test.
            if to_resolve.is_empty() {
                break;
            }

            // If this test has been resolved, skip it.
            let test_name = to_resolve.remove(0);
            if resolved.get(&test_name).is_some() {
                continue;
            }
            resolved.insert(test_name.clone(), ());

            let ref mut test = match loaded_tests.get(&test_name) {
                None => {
                    ts.debug("scenario", id, format!("Test {} not found when loading scenario", test_name).as_str());
                    return Err(ScenarioError::TestNotFound(test_name.clone()));
                },
                Some(s) => s.lock().unwrap(),
            };

            // Add an edge for every test requirement.
            for requirement in test.requirements() {
                to_resolve.push(requirement.clone());
                if let Err(e) = test_graph.add_edge(node_bucket[requirement],
                                                    node_bucket[&test_name],
                                                    TestEdge) {
                    ts.debug("scenario",
                            id,
                            format!("Test {} has a circular requirement on {}",
                                    test_name, requirement).as_str());
                    return Err(ScenarioError::CircularDependency(test_name.clone(), requirement.clone()));
                }
            }

            // Also add an edge for every test suggestion.
            for requirement in test.suggestions() {
                to_resolve.push(requirement.clone());
                if let Err(e) = test_graph.add_edge(node_bucket[requirement],
                                                    node_bucket[&test_name],
                                                    TestEdge) {
                    ts.debug("scenario",
                            id,
                            format!("Warning: test {} has a circular suggestion for {}",
                                    test_name, requirement).as_str());
                }
            }
        }

        let mut seen_nodes = HashMap::new();
        let some_node = node_bucket.get(&test_names[0]).unwrap();
        let mut test_order = vec![];
        Scenario::visit_node(&mut seen_nodes,
                             loaded_tests,
                             some_node,
                             &test_graph,
                             &node_bucket,
                             &mut test_order);

        let vec_names: Vec<String> = test_order.iter().map(|x| x.lock().unwrap().deref().id()).collect();
        ts.debug("scenario", id, format!("Vector order: {:?}", vec_names).as_str());
        Ok(test_order)
    }

    // Start running a scenario
    pub fn start(&self) {
        let controller = self.controller.lock().unwrap();
        controller.send_broadcast(self.id(),
                                  self.kind(),
                                  BroadcastMessageContents::Start(self.id()));
    }

    // Broadcast a description of ourselves.
    pub fn describe(&self) {
        let controller = self.controller.lock().unwrap();
        controller.send_broadcast(self.id(),
                                  self.kind(),
                                  BroadcastMessageContents::Describe(self.kind(),
                                                          "name".to_string(),
                                                          self.id(),
                                                          self.name()));
        controller.send_broadcast(self.id(),
                                  self.kind(),
                                  BroadcastMessageContents::Describe(self.kind(),
                                                            "description".to_string(),
                                                            self.id(),
                                                            self.description()));

        let test_names: Vec<String> = self.tests.iter().map(|x| x.lock().unwrap().deref().id()).collect();

        controller.send_broadcast(self.id(),
                                  self.kind(),
                                  BroadcastMessageContents::Tests(self.id(),
                                                                  test_names));
    }

    pub fn kind(&self) -> String {
        "scenario".to_string()
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn description(&self) -> String {
        self.description.clone()
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }
}