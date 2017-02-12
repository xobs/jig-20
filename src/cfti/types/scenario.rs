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
use super::super::controller::{self, BroadcastMessageContents, ControlMessageContents};

#[derive(Debug)]
pub enum ScenarioError {
    FileLoadError,
    MissingScenarioSection,
    TestListNotFound,
    TestNotFound(String),
    TestDependencyNotFound(String, String),
    CircularDependency(String, String),
}

struct GraphResult {
    graph: Dag<String, TestEdge>,
    node_bucket: HashMap<String, NodeIndex>,
    tests: Vec<Arc<Mutex<Test>>>,
}

#[derive(Debug)]
enum ScenarioState {
    /// The scenario has been loaded, and is ready to run.
    Idle,

    /// The scenario has started, but is waiting for ExecStart to finish
    PreStart,

    /// The scenario is running, and is on step (u32)
    Running(u32),

    /// The scenario has succeeded, and is running the ExecStopSuccess step
    PostSuccess,

    /// The scenario has failed, and is running the ExecStopFailure step
    PostFailure,
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

    /// tests: A vector containing all the tests in this scenario.
    pub tests: Vec<Arc<Mutex<Test>>>,

    /// exec_start: A command to run when starting tests.
    exec_start: Option<String>,

    /// exec_stop_success: A command to run upon successful completion of this scenario.
    exec_stop_success: Option<String>,

    /// exec_stop_failure: A command to run if this scenario fails.
    exec_stop_failure: Option<String>,

    /// The controller where messages go.
    controller: Arc<Mutex<controller::Controller>>,

    /// What the current state of the scenario is.
    state: Arc<Mutex<ScenarioState>>,

    /// How many tests have failed.
    failures: Arc<Mutex<u32>>,

    // These should come in handy, I think.
    graph: Dag<String, TestEdge>,
    node_bucket: HashMap<String, NodeIndex>,
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

        let graph_result = match Self::build_graph(ts, id, &test_names, &loaded_tests) {
            Err(e) => return Some(Err(e)),
            Ok(v) => v,
        };

        Some(Ok(Scenario {
            id: id.to_string(),
            tests: graph_result.tests,
            timeout: timeout,
            name: name,
            description: description,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,
            controller: controller,
            state: Arc::new(Mutex::new(ScenarioState::Idle)),
            failures: Arc::new(Mutex::new(0)),
            graph: graph_result.graph,
            node_bucket: graph_result.node_bucket,
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
            Self::visit_node(seen_nodes,
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
            Self::visit_node(seen_nodes,
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
                   -> Result<GraphResult, ScenarioError> {

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
        let mut test_order = vec![];
        {
            let some_node = node_bucket.get(&test_names[0]).unwrap();
            Self::visit_node(&mut seen_nodes,
                            loaded_tests,
                            some_node,
                            &test_graph,
                            &node_bucket,
                            &mut test_order);
        }
        let vec_names: Vec<String> = test_order.iter().map(|x| x.lock().unwrap().deref().id().to_string()).collect();
        ts.debug("scenario", id, format!("Vector order: {:?}", vec_names).as_str());
        Ok(GraphResult {
            tests: test_order,
            graph: test_graph,
            node_bucket: node_bucket,
        })
    }

    fn find_next_state(&self) -> ScenarioState {
        match self.state.lock().unwrap().deref() {
            &ScenarioState::Idle => {
                // Reset the number of errors.
                *(self.failures.lock().unwrap()) = 0;

                let controller = self.controller.lock().unwrap();
                controller.send_broadcast(self.id(),
                                        self.kind(),
                                        BroadcastMessageContents::Start(self.id().to_string()));

                // If there's a preroll script, run that
                if let Some(ref s) = self.exec_start {
                    ScenarioState::PreStart

                // If there are no tests, jump straight to the end
                } else if self.tests.is_empty() {
                    // Run a post-execution success program, if present
                    if self.exec_stop_success.is_some() {
                        ScenarioState::PostSuccess
                    }
                    else {
                        ScenarioState::Idle
                    }
                }

                // Start running test 0
                else {
                    ScenarioState::Running(0)
                }
            },

            // If we've just run the PreStart command, see if we need
            // to run test 0, or skip straight to Success.
            &ScenarioState::PreStart =>
                if self.tests.is_empty() {
                    if self.exec_stop_success.is_some() {
                        ScenarioState::PostSuccess
                    }
                    else {
                        ScenarioState::Idle
                    }
                }
                else {
                    ScenarioState::Running(0)
                },

            // If we just finished running a test, determine the next test to run.
            &ScenarioState::Running(i) => {
                let ref current_test = self.tests[i as usize].lock().unwrap();
                ScenarioState::Running(i + 1)
            },
            &ScenarioState::PostFailure => ScenarioState::Idle,
            &ScenarioState::PostSuccess => ScenarioState::Idle,
        }
    }

    // Given the current state, figure out the next test to run (if any)
    fn start_next_test(&self) {
        let new_state = self.find_next_state();

        let failures = *(self.failures.lock().unwrap());
        match new_state {
            // If we're transitioning to the idle state, it means we just finished
            // running some tests.  Broadcast the result.
            ScenarioState::Idle => {
                if failures > 0 {
                    self.broadcast(BroadcastMessageContents::Finish(self.id().to_string(),
                                                                    failures + 500,
                                                                    "At least one test failed".to_string()));
                }
                else {
                    self.broadcast(BroadcastMessageContents::Finish(self.id().to_string(),
                                                                    200,
                                                                    "Finished tests".to_string()));
                }
            },

            // If we want to run a preroll command and it fails, log it and start the tests.
            ScenarioState::PreStart => {
                // unwrap is safe because we know a PreStart command exists.
                if let Some(ref cmd) = self.exec_start {
                    if let Err(e) = self.run_command(cmd.clone(),
                                                    ControlMessageContents::AdvanceScenario) {
                        self.log(format!("Unable to run ExecPre command: {:?}", e).as_str());
                        self.start_next_test();
                    }
                }
            },
            ScenarioState::Running(next_step) => (),
            ScenarioState::PostSuccess => (),
            ScenarioState::PostFailure => (),
        }
    }

    fn run_command(&self, command: String, finish_message: ControlMessageContents) -> Result<(), ScenarioError> {
/*
        let mut cmd = match process::make_command(self.exec_start.as_str()) {
            Ok(s) => s,
            Err(e) => { ts.debug("interface", self.id.as_str(), format!("Unable to run logger: {:?}", e).as_str()); return Err(InterfaceError::MakeCommandFailed) },
        };
        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        match self.working_directory {
            None => (),
            Some(ref s) => {cmd.current_dir(s); },
        }

        let child = match cmd.spawn() {
            Err(e) => { println!("Unable to spawn {:?}: {}", cmd, e); return Err(InterfaceError::ExecCommandFailed) },
            Ok(s) => s,
        };
*/
        Ok(())
    }

    // Start running a scenario
    pub fn start(&self) {
        self.start_next_test();
    }

    // Broadcast a description of ourselves.
    pub fn describe(&self) {
        self.broadcast(BroadcastMessageContents::Describe(self.kind().to_string(),
                                                          "name".to_string(),
                                                          self.id().to_string(),
                                                          self.name().to_string()));
        self.broadcast(BroadcastMessageContents::Describe(self.kind().to_string(),
                                                          "description".to_string(),
                                                          self.id().to_string(),
                                                          self.description().to_string()));

        let test_names: Vec<String> = self.tests.iter().map(|x| x.lock().unwrap().deref().id().to_string()).collect();

        self.broadcast(BroadcastMessageContents::Tests(self.id().to_string(), test_names));
    }

    pub fn kind(&self) -> &str {
        "scenario"
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn description(&self) -> &str {
        self.description.as_str()
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    fn broadcast(&self, msg: BroadcastMessageContents) {
        let controller = self.controller.lock().unwrap();
        controller.send_broadcast(self.id(), self.kind(), msg);
    }

    fn log(&self, msg: &str) {
        self.broadcast(BroadcastMessageContents::Log(msg.to_string()));
    }
}