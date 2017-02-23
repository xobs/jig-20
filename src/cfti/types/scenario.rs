extern crate ini;
extern crate daggy;
extern crate bus;

use self::ini::Ini;
use self::daggy::{Dag, Walker, NodeIndex};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::ops::Deref;
use std::ops::DerefMut;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use std::thread;
use std::time;

use cfti::types::test::Test;
use cfti::types::Jig;
use cfti::process;
use cfti::controller::{Controller, BroadcastMessageContents, ControlMessageContents};

const DEFAULT_TIMEOUT: u32 = (60 * 60 * 24);

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, PartialEq)]
/// If a test has no TestResult, then it is considered Pending.
enum TestResult {
    /// The test has started, and is currently running
    Running,

    /// The test ended successfully
    Success,

    /// The test failed
    Failure,

    /// The test was skipped for some reason
    Skipped,
}

#[derive(Clone, Debug, PartialEq)]
enum ScenarioState {
    /// The scenario has been loaded, and is ready to run.
    Idle,

    /// The scenario has started, but is waiting for ExecStart to finish
    PreStart,

    /// The scenario is running, and is on step (u32)
    Running(usize),

    /// The scenario has succeeded, and is running the ExecStopSuccess step
    PostSuccess,

    /// The scenario has failed, and is running the ExecStopFailure step
    PostFailure,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum TestEdge {
    /// Test B Requires test A, and a failure of A prevents B from running
    Requires,

    /// Test B Suggests test A, and a failure of A doesn't prevent B from running
    Suggests,

    /// Test B follows test A in the .scenario file
    Follows,
}

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

    /// The controller where all messages come and go.
    controller: Controller,

    /// What the current state of the scenario is.
    state: Arc<Mutex<ScenarioState>>,

    /// How many tests have failed.
    failures: Arc<Mutex<u32>>,

    /// The result of various tests, indexed by test name.
    results: Arc<Mutex<HashMap<String, TestResult>>>,

    /// Dependency graph for all tests to be run.
    graph: Dag<String, TestEdge>,

    /// A hashmap containing all nodes in the graph, indexed by name.
    node_bucket: HashMap<String, NodeIndex>,

    /// The default directory for all tests during this test run.
    working_directory: Arc<Mutex<Option<String>>>,

    /// The timestamp when the test started, used to calculate timeouts.
    start_time: Arc<Mutex<time::Instant>>,
}

impl Scenario {
    pub fn new(id: &str,
               path: &str,
               loaded_jigs: &HashMap<String, Arc<Mutex<Jig>>>,
               loaded_tests: &HashMap<String, Arc<Mutex<Test>>>,
               controller: &Controller) -> Option<Result<Scenario, ScenarioError>> {

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
                    controller.debug("scenario", id, format!("The scenario '{}' is not compatible with this jig", id));
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
            None => DEFAULT_TIMEOUT,
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

        let graph_result = match Self::build_graph(controller, id, &test_names, &loaded_tests) {
            Err(e) => return Some(Err(e)),
            Ok(v) => v,
        };

        let failures = Arc::new(Mutex::new(0));

        let test_results = Arc::new(Mutex::new(HashMap::new()));
        let thr_results = test_results.clone();
        let thr_failures = failures.clone();

        // Monitor broadcast states to determine when tests finish.
        controller.listen(move |msg| {
            let mut results = thr_results.lock().unwrap();
            match msg.message {
                BroadcastMessageContents::Skip(test, _) => results.insert(test, TestResult::Skipped),
                BroadcastMessageContents::Pass(test, _) => results.insert(test, TestResult::Success),
                BroadcastMessageContents::Running(test) => results.insert(test, TestResult::Running),
                BroadcastMessageContents::Fail(test, _) => {
                    let mut failures = thr_failures.lock().unwrap();
                    *failures = *failures + 1;
                    results.insert(test, TestResult::Failure)
                },
                _ => None,
            };
        });

        Some(Ok(Scenario {
            id: id.to_string(),
            tests: graph_result.tests,
            timeout: timeout,
            name: name,
            description: description,
            exec_start: exec_start,
            exec_stop_success: exec_stop_success,
            exec_stop_failure: exec_stop_failure,
            controller: controller.clone(),
            state: Arc::new(Mutex::new(ScenarioState::Idle)),
            failures: failures,
            results: test_results,
            graph: graph_result.graph,
            node_bucket: graph_result.node_bucket,
            working_directory: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(time::Instant::now())),
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
        for (_, parent_index) in parents.iter(test_graph) {
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
        for (_, child_index) in children.iter(test_graph) {
            Self::visit_node(seen_nodes,
                             loaded_tests,
                             &child_index,
                             test_graph,
                             node_bucket,
                             test_order);
        }
    }

    fn build_graph(controller: &Controller,
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

        // Add a dependency on the graph to indicate the order of tests.
        {
            let num_tests = test_names.len();
            for i in 1 .. num_tests {
                let previous_test = test_names[i - 1].clone();
                let this_test = test_names[i].clone();
                if let Err(_) = test_graph.add_edge(*(node_bucket.get(&previous_test).unwrap()),
                                                    *(node_bucket.get(&this_test).unwrap()),
                                                    TestEdge::Follows) {
                    controller.debug("scenario",
                            id,
                            format!("Test {} has a circular requirement on {}",
                                    test_names[i - 1], test_names[i]));
                    return Err(ScenarioError::CircularDependency(test_names[i - 1].clone(), test_names[i].clone()));
                }
            }
        }

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
                    controller.debug("scenario", id, format!("Test {} not found when loading scenario", test_name));
                    return Err(ScenarioError::TestNotFound(test_name.clone()));
                },
                Some(s) => s.lock().unwrap(),
            };

            // Add an edge for every test requirement.
            for requirement in test.requirements() {
                to_resolve.push(requirement.clone());
                let edge = match node_bucket.get(requirement) {
                    None => {
                        controller.debug("scenario",
                                id,
                                format!("Test {} has a requirement that doesn't exist: {}",
                                        test_name, requirement));
                        return Err(ScenarioError::TestDependencyNotFound(test_name, requirement.to_string()));
                    },
                    Some(e) => e,
                };
                if let Err(_) = test_graph.add_edge(*edge,
                                                    node_bucket[&test_name],
                                                    TestEdge::Requires) {
                    controller.debug("scenario",
                            id,
                            format!("Test {} has a circular requirement on {}",
                                    test_name, requirement));
                    return Err(ScenarioError::CircularDependency(test_name.clone(), requirement.clone()));
                }
            }

            // Also add an edge for every test suggestion.
            for requirement in test.suggestions() {
                to_resolve.push(requirement.clone());
                let edge = match node_bucket.get(requirement) {
                    None => {
                        controller.debug("scenario",
                                id,
                                format!("Test {} has a dependency that doesn't exist: {}",
                                        test_name, requirement));
                        return Err(ScenarioError::TestDependencyNotFound(test_name, requirement.to_string()));
                    },
                    Some(e) => e,
                };
                if let Err(_) = test_graph.add_edge(*edge,
                                                    node_bucket[&test_name],
                                                    TestEdge::Suggests) {
                    controller.debug("scenario",
                            id,
                            format!("Warning: test {} has a circular suggestion for {}",
                                    test_name, requirement));
                }
            }
        }

        let mut seen_nodes = HashMap::new();
        let mut test_order = vec![];
        {
            // Pick a node from the bucket and visit it.  This will cause
            // all nodes in the graph to be visited, in order.
            let some_node = node_bucket.get(&test_names[0]).unwrap();
            Self::visit_node(&mut seen_nodes,
                            loaded_tests,
                            some_node,
                            &test_graph,
                            &node_bucket,
                            &mut test_order);
        }
        let vec_names: Vec<String> = test_order.iter().map(|x| x.lock().unwrap().deref().id().to_string()).collect();
        controller.debug("scenario", id, format!("Vector order: {:?}", vec_names));
        Ok(GraphResult {
            tests: test_order,
            graph: test_graph,
            node_bucket: node_bucket,
        })
    }

    fn all_dependencies_succeeded(&self, test_name: &String) -> bool {
        let parents = self.graph.parents(self.node_bucket[test_name]);

        for (edge, node) in parents.iter(&self.graph) {
            // We're only interested in parents that are required.
            if *(self.graph.edge_weight(edge).unwrap()) != TestEdge::Requires {
                continue;
            }

            let parent_name = self.graph.node_weight(node).unwrap();
            let result = {
                // Borrow the results hashmap
                let results = self.results.lock().unwrap();
                // If the test has no result, it hasn't been run,
                // and so therefore did not succeed.
                match results.get(parent_name) {
                    None => return false,
                    Some(s) => s.clone(),
                }
            };

            // If the dependent test did not succeed, then at least
            // one dependency failed.
            if result != TestResult::Success {
                return false;
            }

            if !self.all_dependencies_succeeded(parent_name) {
                return false;
            }
        }
        true
    }

    // Check the proposed state to make sure it's acceptable.
    // Reasons it might not be acceptable might be because there
    // is no exec_start and the new state is PreStart, or because
    // the new state is on a test whose requirements are not met.
    fn is_state_okay(&self, new_state: &ScenarioState) -> bool {

        match *new_state {
            // We can always enter the idle state.
            ScenarioState::Idle => true,

            // Run an exec_start command before we run the first test.
            ScenarioState::PreStart => {
                // If there's a preroll script, run that.
                if self.exec_start.is_some() {
                    true
                }
                else {
                    false
                }
            },

            // Run a given test.
            ScenarioState::Running(i) => {
                let test_name = self.tests[i].lock().unwrap().id().to_string();
                if i >= self.tests.len() {
                    false
                }
                // Make sure all required dependencies succeeded.
                else if ! self.all_dependencies_succeeded(&test_name) {
                    self.results.lock().unwrap().insert(test_name.clone(), TestResult::Skipped);
                    self.broadcast(BroadcastMessageContents::Skip(test_name.clone(), "dependency failed".to_string()));
                    false
                }
                else {
                    true
                }
            },

            // Run a script on scenario success.
            ScenarioState::PostSuccess => {
                if self.exec_stop_success.is_some() {
                    true
                }
                else {
                    false
                }
            },

            // Run a script on scenario failure.
            ScenarioState::PostFailure => {
                if self.exec_stop_failure.is_some() {
                    true
                }
                else {
                    false
                }
            },
        }
    }

    /* Find the next state.
     * If we're idle, start the test.
     * The state order goes:
     * Idle -> [PreStart] -> Test(0) -> ... -> Test(n) -> [PostSuccess/Fail] -> Idle
     */
    fn find_next_state(&self, current_state: ScenarioState) -> ScenarioState {
        
        let test_count = self.tests.len();
        let failure_count = {
            *(self.failures.lock().unwrap())
        };

        let new_state = match current_state {
            ScenarioState::Idle => {
                // Reset the number of errors.
                *(self.failures.lock().unwrap()) = 0;
                self.results.lock().unwrap().clear();

                self.controller.broadcast(
                                      self.id(),
                                      self.kind(),
                                      &BroadcastMessageContents::Start(self.id().to_string()));
                ScenarioState::PreStart
            },

            // If we've just run the PreStart command, see if we need
            // to run test 0, or skip straight to Success.
            ScenarioState::PreStart => ScenarioState::Running(0),

            // If we just finished running a test, determine the next test to run.
            ScenarioState::Running(i) if (i + 1) < test_count => ScenarioState::Running(i + 1),
            ScenarioState::Running(i) if (i + 1) >= test_count && failure_count > 0 => ScenarioState::PostFailure,
            ScenarioState::Running(i) if (i + 1) >= test_count && failure_count == 0 => ScenarioState::PostSuccess,
            ScenarioState::Running(i) => panic!("Got into a weird state. Running({}), test_count: {}, failure_count: {}", i, test_count, failure_count),
            ScenarioState::PostFailure => ScenarioState::Idle,
            ScenarioState::PostSuccess => ScenarioState::Idle,
        };

        // If it's an acceptable new state, set that.  Otherwise, recurse
        // and try the next state.
        if self.is_state_okay(&new_state) {
            *(self.state.lock().unwrap().deref_mut()) = new_state.clone();
            new_state
        }
        else {
            self.find_next_state(new_state)
        }
    }

    fn run_support_cmd(&self, cmd: String, testname: String) {
        // unwrap is safe because we know a PreStart command exists.
        let id = self.id().to_string();
        let kind = self.kind().to_string();
        let tn = testname.clone();
        let controller = self.controller.clone();
        let res = process::try_command_completion(cmd.as_str(),
                                        self.working_directory.lock().unwrap().deref(),
                                        Duration::new(100, 0),
                                        move |res: Result<(), process::CommandError>| {
            let msg = match res {
                Ok(_) => BroadcastMessageContents::Pass(tn, "".to_string()),
                Err(e) => BroadcastMessageContents::Fail(tn, format!("{:?}", e)),
            };

            // Send a message indicating what the test did, and advance the scenario.
            controller.broadcast_class("support", id.as_str(), kind.as_str(), &msg);
            controller.control_class(
                "support",
                id.as_str(),
                kind.as_str(),
                &ControlMessageContents::AdvanceScenario);
        });

        // The command will either return an error, or a tuple containing (stdout,stdin).
        // If it's an error, then the completion above will be called and the test state
        // will be advanced there.  Avoid advancing it here.
        let (stdout, _) = match res {
            Err(_) => return,
            Ok(s) => s,
        };

        let controller = self.controller.clone();
        let id = self.id().to_string();
        let kind = self.kind().to_string();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Err(_) => {
                        /* Support command ended */
                        return;
                    },
                    Ok(l) => {
                        controller.broadcast_class(
                            "support",
                            id.as_str(),
                            kind.as_str(),
                            &BroadcastMessageContents::Log(format!("{}: {}", testname, l)));
                    },
                }
            }
        });
    }

    // Given the current state, figure out the next test to run (if any)
    pub fn advance(&self) {
        let current_state = self.state.lock().unwrap().clone();
        let new_state = self.find_next_state(current_state);

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
                let ref cmd = self.exec_start;
                let cmd = cmd.clone().unwrap().clone();
                self.run_support_cmd(cmd, "execstart".to_string());
            },
            ScenarioState::Running(next_step) => {
                let ref test = self.tests[next_step].lock().unwrap();
                test.start(self.working_directory.lock().unwrap().deref());
            },
            ScenarioState::PostSuccess => {
                let ref cmd = self.exec_stop_success;
                let cmd = cmd.clone().unwrap().clone();
                self.run_support_cmd(cmd, "execstart".to_string());
            },
            ScenarioState::PostFailure => {
                let ref cmd = self.exec_stop_failure;
                let cmd = cmd.clone().unwrap().clone();
                self.run_support_cmd(cmd, "execstart".to_string());
            },
        }
    }

    /// Start running a scenario
    ///
    /// Start running a scenario.  If `working_directory` is specified,
    /// then use that for all tests that don't specify one.
    pub fn start(&self, working_directory: &Option<String>) {
        {
            let current_state = self.state.lock().unwrap().clone();
            if current_state != ScenarioState::Idle {
                self.log(format!("NOT starting new scenario run because ScenarioState is {:?}, not Idle", current_state));
                return;
            }
            self.log("Starting new scenario run".to_string());

            // Save the current instant, so we can timeout as needed.
            *(self.start_time.lock().unwrap()) = time::Instant::now();
        }
        *(self.working_directory.lock().unwrap()) = working_directory.clone();
        self.advance();
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
        self.controller.broadcast(self.id(), self.kind(), &msg);
    }

    fn log(&self, msg: String) {
        self.broadcast(BroadcastMessageContents::Log(msg));
    }
}