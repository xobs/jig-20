extern crate daggy;
extern crate bus;

use self::daggy::{Dag, Walker, NodeIndex};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time;

use cfti::types::test::{Test, TestState};
use cfti::types::Unit;
use cfti::process;
use cfti::config;
use cfti::testset;
use cfti::controller::{Controller, BroadcastMessageContents, ControlMessageContents};
use cfti::unitfile::UnitFile;

#[derive(Clone, Debug)]
pub enum ScenarioError {
    FileLoadError(String),
    MissingScenarioSection,
    TestListNotFound,
    TestNotFound(String),
    TestDependencyNotFound(String, String),
    CircularDependency(String, String),
    MissingDependency(String, String),
}

struct GraphResult {
    graph: Dag<String, TestEdge>,
    node_bucket: HashMap<String, NodeIndex>,
    tests: Vec<Arc<Mutex<Test>>>,
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

    /// The test has succeeded or failed
    TestFinished,
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
    timeout: Duration,

    /// tests: A vector containing all the tests in this scenario.
    pub tests: Vec<Arc<Mutex<Test>>>,

    /// A map of test names to test indexes.
    tests_map: HashMap<String, usize>,

    /// exec_start: A command to run when starting tests.
    exec_start: Option<String>,

    /// How long to wait for exec_start to run
    exec_start_timeout: Duration,

    /// exec_stop_success: A command to run upon successful completion of this scenario.
    exec_stop_success: Option<String>,

    /// How long to wait for exec_stop_success to run
    exec_stop_success_timeout: Duration,

    /// exec_stop_failure: A command to run if this scenario fails.
    exec_stop_failure: Option<String>,

    /// How long to wait for exec_stop_failure to run
    exec_stop_failure_timeout: Duration,

    /// The controller where all messages come and go.
    controller: Controller,

    /// What the current state of the scenario is.
    state: Arc<Mutex<ScenarioState>>,

    /// How many tests have failed.
    failures: Arc<Mutex<u32>>,

    /// Dependency graph for all tests to be run.
    graph: Dag<String, TestEdge>,

    /// A hashmap containing all nodes in the graph, indexed by name.
    node_bucket: HashMap<String, NodeIndex>,

    /// The default directory for all tests during this test run.
    working_directory: Arc<Mutex<Option<String>>>,

    /// The timestamp when the test started, used to calculate timeouts.
    start_time: Arc<Mutex<time::Instant>>,

    /// Currently-running child support command.
    support_cmd: Arc<Mutex<Option<process::ChildProcess>>>,
}

impl Scenario {
    pub fn new(id: &str,
               path: &str,
               test_set: &testset::TestSet,
               config: &config::Config)
               -> Option<Result<Scenario, ScenarioError>> {

        let loaded_jigs = test_set.jigs();
        let loaded_tests = test_set.tests();

        // Load the .ini file
        let unitfile = match UnitFile::new(path) {
            Err(e) => return Some(Err(ScenarioError::FileLoadError(format!("{:?}", e)))),
            Ok(s) => s,
        };

        if !unitfile.has_section("Scenario") {
            return Some(Err(ScenarioError::MissingScenarioSection));
        }

        // Check to see if this scenario is compatible with this jig.
        match unitfile.get("Scenario", "Jigs") {
            None => (),
            Some(s) => {
                let jig_names: Vec<String> =
                    s.split(|c| c == ',' || c == ' ').map(|s| s.to_string()).collect();
                let mut found_it = false;
                for jig_name in jig_names {
                    if loaded_jigs.get(&jig_name).is_some() {
                        found_it = true;
                        break;
                    }
                }
                if found_it == false {
                    test_set.debug(format!("The scenario '{}' is not compatible with this jig",
                                             id));
                    return None;
                }
            }
        }

        let description = match unitfile.get("Scenario", "Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match unitfile.get("Scenario", "Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let timeout = match unitfile.get("Scenario", "Timeout") {
            None => config.scenario_timeout(),
            Some(s) => time::Duration::from_secs(s.parse().unwrap()),
        };

        let exec_start = match unitfile.get("Scenario", "ExecStart") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_success = match unitfile.get("Scenario", "ExecStopSuccess") {
            None => {
                match unitfile.get("Scenario", "ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                }
            }
            Some(s) => Some(s.to_string()),
        };

        let exec_stop_failure = match unitfile.get("Scenario", "ExecStopFail") {
            None => {
                match unitfile.get("Scenario", "ExecStop") {
                    None => None,
                    Some(s) => Some(s.to_string()),
                }
            }
            Some(s) => Some(s.to_string()),
        };

        let test_names = match unitfile.get("Scenario", "Tests") {
            None => return Some(Err(ScenarioError::TestListNotFound)),
            // Split by "," and also whitespace, and combine back into an array.
            Some(s) => {
                s.split(",")
                    .map(|x| {
                        x.to_string()
                            .split_whitespace()
                            .map(|y| y.to_string().trim().to_string())
                            .collect()
                    })
                    .collect()
            }
        };

        let graph_result = match Self::build_graph(test_set, &test_names, &loaded_tests) {
            Err(e) => return Some(Err(e)),
            Ok(v) => v,
        };

        let vec_names: Vec<String> =
            graph_result.tests.iter().map(|x| x.lock().unwrap().id().to_string()).collect();
        test_set.debug(format!("Scenario {} vector order: {:?}", id, vec_names));

        let mut test_map = HashMap::new();
        for (idx, test) in graph_result.tests.iter().enumerate() {
            test_map.insert(test.lock().unwrap().id().to_string(), idx);
        }

        let failures = Arc::new(Mutex::new(0));

        let thr_failures = failures.clone();

        // Monitor broadcast states to determine when tests finish.
        test_set.controller().listen(move |msg| {
            match msg.message {
                BroadcastMessageContents::Fail(_, _) => {
                    let mut failures = thr_failures.lock().unwrap();
                    *failures = *failures + 1;
                }
                _ => (),
            };
            Ok(())
        });

        Some(Ok(Scenario {
            id: id.to_string(),
            tests: graph_result.tests,
            tests_map: test_map,
            timeout: timeout,
            name: name,
            description: description,
            exec_start: exec_start,
            exec_start_timeout: config.scenario_start_timeout(),
            exec_stop_success: exec_stop_success,
            exec_stop_success_timeout: config.scenario_failure_timeout(),
            exec_stop_failure: exec_stop_failure,
            exec_stop_failure_timeout: config.scenario_success_timeout(),
            controller: test_set.controller().clone(),
            state: Arc::new(Mutex::new(ScenarioState::Idle)),
            failures: failures,
            graph: graph_result.graph,
            node_bucket: graph_result.node_bucket,
            working_directory: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(time::Instant::now())),
            support_cmd: Arc::new(Mutex::new(None)),
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

        // 1. Visit all parents
        // 2. Visit ourselves
        // 3. Visit all children
        // Build the nodes into a vec
        //

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

    fn build_graph<T: Unit>(unit: &T,
                            test_names: &Vec<String>,
                            loaded_tests: &HashMap<String, Arc<Mutex<Test>>>)
                            -> Result<GraphResult, ScenarioError> {

        // Resolve the test names.
        let mut test_graph = Dag::<String, TestEdge>::new();
        let mut node_bucket = HashMap::new();

        // Create a node for each available test.  We will add
        // edges later on as we traverse the dependency lists.
        for (test_name, _) in loaded_tests {
            node_bucket.insert(test_name.clone(), test_graph.add_node(test_name.clone()));
        }

        let mut to_resolve = test_names.clone();

        // Add a dependency on the graph to indicate the order of tests.
        {
            let num_tests = test_names.len();
            for i in 1..num_tests {
                let previous_test = test_names[i - 1].clone();
                let this_test = test_names[i].clone();
                let previous_edge = match node_bucket.get(&previous_test) {
                    Some(s) => s,
                    None => {
                        unit.debug(format!("Previous test {} could not be found in the \
                                                  node bucket",
                                           previous_test));
                        return Err(ScenarioError::MissingDependency(this_test, previous_test));
                    }
                };
                let this_edge = match node_bucket.get(&this_test) {
                    Some(s) => s,
                    None => {
                        unit.debug(format!("This test {} could not be found in the node \
                                                  bucket",
                                           this_test));
                        return Err(ScenarioError::MissingDependency(this_test, previous_test));
                    }
                };
                if let Err(_) = test_graph.add_edge(*previous_edge, *this_edge, TestEdge::Follows) {
                    unit.debug(format!("Test {} has a circular requirement on {}",
                                       test_names[i - 1],
                                       test_names[i]));
                    return Err(ScenarioError::CircularDependency(test_names[i - 1].clone(),
                                                                 test_names[i].clone()));
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
                    unit.debug(format!("Test {} not found when loading scenario", test_name));
                    return Err(ScenarioError::TestNotFound(test_name.clone()));
                }
                Some(s) => s.lock().unwrap(),
            };

            // Add an edge for every test requirement.
            for requirement in test.requirements() {
                to_resolve.push(requirement.clone());
                let edge = match node_bucket.get(requirement) {
                    None => {
                        unit.debug(format!("Test {} has a requirement that doesn't exist: \
                                                  {}",
                                           test_name,
                                           requirement));
                        return Err(ScenarioError::TestDependencyNotFound(test_name,
                                                                         requirement.to_string()));
                    }
                    Some(e) => e,
                };
                if let Err(_) =
                       test_graph.add_edge(*edge, node_bucket[&test_name], TestEdge::Requires) {
                    unit.debug(format!("Test {} has a circular requirement on {}",
                                       test_name,
                                       requirement));
                    return Err(ScenarioError::CircularDependency(test_name.clone(),
                                                                 requirement.clone()));
                }
            }

            // Also add an edge for every test suggestion.
            for requirement in test.suggestions() {
                to_resolve.push(requirement.clone());
                let edge = match node_bucket.get(requirement) {
                    None => {
                        unit.debug(format!("Test {} has a dependency that doesn't exist: \
                                                  {}",
                                           test_name,
                                           requirement));
                        return Err(ScenarioError::TestDependencyNotFound(test_name,
                                                                         requirement.to_string()));
                    }
                    Some(e) => e,
                };
                if let Err(_) =
                       test_graph.add_edge(*edge, node_bucket[&test_name], TestEdge::Suggests) {
                    unit.debug(format!("Warning: test {} has a circular suggestion for {}",
                                       test_name,
                                       requirement));
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
                self.tests[self.tests_map[parent_name]].lock().unwrap().state()
            };

            // If the dependent test did not succeed, then at least
            // one dependency failed.
            // The test may also be Running, in case it's a Daemon.
            if result != TestState::Pass && result != TestState::Running {
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
            ScenarioState::PreStart => self.exec_start.is_some(),

            // Run a given test.
            ScenarioState::Running(i) => {
                let test_name = self.tests[i].lock().unwrap().id().to_string();
                if self.scenario_timed_out() {
                    false
                } else if i >= self.tests.len() {
                    false
                }
                // If the test isn't Pending (i.e. if it's skipped or failed), don't run it.
                else if self.tests[i].lock().unwrap().state() != TestState::Pending {
                    false
                }
                // Make sure all required dependencies succeeded.
                else if !self.all_dependencies_succeeded(&test_name) {
                    self.tests[i].lock().unwrap().skip();
                    self.broadcast(BroadcastMessageContents::Skip(test_name.clone(),
                                                                  "dependency failed".to_string()));
                    false
                } else {
                    true
                }
            }

            // Run a script on scenario success.
            ScenarioState::PostSuccess => self.exec_stop_success.is_some(),

            // Run a script on scenario failure.
            ScenarioState::PostFailure => self.exec_stop_failure.is_some(),

            // Presumably we can always finish a test.
            ScenarioState::TestFinished => true,
        }
    }

    // Find the next state.
    // If we're idle, start the test.
    // The state order goes:
    // Idle -> [PreStart] -> Test(0) -> ... -> Test(n) -> [PostSuccess/Fail] -> Idle
    //
    fn find_next_state(&self, current_state: ScenarioState) -> ScenarioState {

        let test_count = self.tests.len();
        let failure_count = *(self.failures.lock().unwrap());

        let new_state = match current_state {
            ScenarioState::Idle => {
                // Reset the number of errors.
                *(self.failures.lock().unwrap()) = 0;
                for test in &self.tests {
                    test.lock().unwrap().pending();
                }

                self.broadcast(BroadcastMessageContents::Start(self.id().to_string()));
                ScenarioState::PreStart
            }

            // If we've just run the PreStart command, see if we need
            // to run test 0, or skip straight to Success.
            ScenarioState::PreStart => ScenarioState::Running(0),

            // If we just finished running a test, determine the next test to run.
            ScenarioState::Running(i) if (i + 1) < test_count => ScenarioState::Running(i + 1),
            ScenarioState::Running(i) if (i + 1) >= test_count && failure_count > 0 => {
                ScenarioState::PostFailure
            }
            ScenarioState::Running(i) if (i + 1) >= test_count && failure_count == 0 => {
                ScenarioState::PostSuccess
            }
            ScenarioState::Running(i) => {
                panic!("Got into a weird state. Running({}), test_count: {}, failure_count: {}",
                       i,
                       test_count,
                       failure_count)
            }
            ScenarioState::PostFailure => ScenarioState::TestFinished,
            ScenarioState::PostSuccess => ScenarioState::TestFinished,
            ScenarioState::TestFinished => ScenarioState::TestFinished,
        };

        // If it's an acceptable new state, set that.  Otherwise, recurse
        // and try the next state.
        if self.is_state_okay(&new_state) {
            *(self.state.lock().unwrap()) = new_state.clone();
            new_state
        } else {
            self.find_next_state(new_state)
        }
    }

    fn run_support_cmd(&self, cmd: &str, timeout: &Duration, testname: &str) {
        // unwrap is safe because we know a PreStart command exists.
        let tn = testname.to_string();
        let unit = self.to_simple_unit();
        let thr_support_cmd = self.support_cmd.clone();
        let res = process::try_command_completion(cmd,
                                                  &*self.working_directory.lock().unwrap(),
                                                  *timeout,
                                                  move |res: Result<(), process::CommandError>| {
            let msg = match res {
                Ok(_) => BroadcastMessageContents::Pass(tn, "".to_string()),
                Err(e) => BroadcastMessageContents::Fail(tn, format!("{:?}", e)),
            };

            *(thr_support_cmd.lock().unwrap()) = None;

            // Send a message indicating what the test did, and advance the scenario.
            Controller::broadcast_class_unit("support", &unit, &msg);
            Controller::control_class_unit("support",
                                           &unit,
                                           &ControlMessageContents::AdvanceScenario);
        });

        // The command will either return an error, or a tuple containing (stdout,stdin).
        // If it's an error, then the completion above will be called and the test state
        // will be advanced there.  Avoid advancing it here.
        let child = match res {
            Err(_) => return,
            Ok(s) => s,
        };

        process::log_output(child.stdout, self, "stdout");
        process::log_output(child.stderr, self, "stderr");
        *(self.support_cmd.lock().unwrap()) = Some(child.child);
    }

    /// Don't run any new tests.  Stop the current test if one is running.
    pub fn abort(&self) {
        let mut current_state = self.state.lock().unwrap();

        match *current_state {
            // Already idle, so nothing to do.
            ScenarioState::Idle |
            ScenarioState::TestFinished => (),

            // Running one of our support commands. Stop that.
            ScenarioState::PreStart |
            ScenarioState::PostFailure |
            ScenarioState::PostSuccess => {
                if let Some(ref cmd) = *(self.support_cmd.lock().unwrap()) {
                    cmd.kill();
                }
                self.finish_scenario();
            }

            // In the middle of running a test.
            ScenarioState::Running(i) => {
                self.tests[i].lock().unwrap().skip();
                for test_num in i..self.tests.len() {
                    self.tests[test_num].lock().unwrap().skip();
                }
                self.tests[i].lock().unwrap().stop(&*self.working_directory.lock().unwrap());
                self.finish_scenario();
            }
        }

        *current_state = ScenarioState::TestFinished;
    }

    // Post messages and terminate tests.
    pub fn finish_scenario(&self) {
        let failures = *(self.failures.lock().unwrap());
        for test in &self.tests {
            test.lock().unwrap().terminate();
        }
        if failures > 0 {
            self.log(format!("{} tests failed", failures));
            self.broadcast(BroadcastMessageContents::Finish(self.id().to_string(),
                                                            failures + 500,
                                                            "At least one test failed"
                                                                .to_string()));
        } else {
            self.log(format!("All tests passed successfully"));
            self.broadcast(BroadcastMessageContents::Finish(self.id().to_string(),
                                                            200,
                                                            "Finished tests".to_string()));
        }
    }

    // Given the current state, figure out the next test to run (if any)
    pub fn advance(&self) {
        let current_state = self.state.lock().unwrap().clone();

        // Run the test's stop() command if we just ran a test.
        match current_state {
            ScenarioState::Running(step) => {
                self.tests[step]
                    .lock()
                    .unwrap()
                    .stop(&*self.working_directory.lock().unwrap())
            }
            _ => (),
        }

        let new_state = self.find_next_state(current_state);

        match new_state {
            // We generally shouldn't transition to the Idle state.
            ScenarioState::Idle => (),

            // If we want to run a preroll command and it fails, log it and start the tests.
            ScenarioState::PreStart => {
                let ref cmd = self.exec_start;
                let cmd = cmd.clone().unwrap();
                self.run_support_cmd(cmd.as_str(), &self.exec_start_timeout, "execstart");
            }
            ScenarioState::Running(next_step) => {
                let ref test = self.tests[next_step].lock().unwrap();
                let test_timeout = test.timeout();
                let test_max_time = self.make_timeout(test_timeout);
                test.start(&*self.working_directory.lock().unwrap(), test_max_time);
            }
            ScenarioState::PostSuccess => {
                let ref cmd = self.exec_stop_success;
                let cmd = cmd.clone().unwrap();
                self.run_support_cmd(cmd.as_str(),
                                     &self.exec_stop_success_timeout,
                                     "execstopsuccess");
            }
            ScenarioState::PostFailure => {
                let ref cmd = self.exec_stop_failure;
                let cmd = cmd.clone().unwrap();
                self.run_support_cmd(cmd.as_str(),
                                     &self.exec_stop_failure_timeout,
                                     "execstopfailure");
            }

            // If we're transitioning to the Finshed state, it means we just finished
            // running some tests.  Broadcast the result.
            ScenarioState::TestFinished => self.finish_scenario(),
        }
    }

    fn scenario_timed_out(&self) -> bool {
        let now = time::Instant::now();
        let scenario_elapsed_time = now.duration_since(self.start_time.lock().unwrap().clone());
        scenario_elapsed_time >= self.timeout
    }

    fn make_timeout(&self, test_max_time: Duration) -> time::Duration {
        let now = time::Instant::now();
        let scenario_elapsed_time = now.duration_since(self.start_time.lock().unwrap().clone());

        // If the test would take longer than the scenario has left, limit the test time.
        if (test_max_time + scenario_elapsed_time) > self.timeout {
            self.timeout - scenario_elapsed_time
        } else {
            test_max_time
        }
    }

    /// Start running a scenario
    ///
    /// Start running a scenario.  If `working_directory` is specified,
    /// then use that for all tests that don't specify one.
    pub fn start(&self, working_directory: &Option<String>) {
        {
            let mut current_state = self.state.lock().unwrap();
            if *current_state != ScenarioState::Idle &&
               *current_state != ScenarioState::TestFinished {
                self.log(format!("NOT starting new scenario run because ScenarioState is {:?}, \
                                  not Idle",
                                 *current_state));
                return;
            }
            self.log("Starting new scenario run".to_string());

            // Reset the results so we can start afresh.
            for test in &self.tests {
                test.lock().unwrap().pending();
            }

            // Save the current instant, so we can timeout as needed.
            *(self.start_time.lock().unwrap()) = time::Instant::now();

            *current_state = ScenarioState::Idle;
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

        let test_names: Vec<String> =
            self.tests.iter().map(|x| x.lock().unwrap().id().to_string()).collect();

        self.broadcast(BroadcastMessageContents::Tests(self.id().to_string(), test_names));
    }
}

impl Unit for Scenario {
    fn kind(&self) -> &str {
        "scenario"
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn description(&self) -> &str {
        self.description.as_str()
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn controller(&self) -> &Controller {
        &self.controller
    }
}