// Copyright (c) 2024 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use common::{
    commands::CompleteState,
    objects::{DeleteCondition, State},
};
use std::collections::HashMap;

use self::cyclic_check::CyclicCheckResult;

mod cyclic_check {
    use super::State;
    use core::fmt;
    use std::collections::{HashSet, VecDeque};

    #[derive(Debug, PartialEq, Eq)]
    pub enum CyclicCheckResult {
        WorkloadPartOfCycle(String),
        InvalidStructure(String),
    }

    impl fmt::Display for CyclicCheckResult {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                CyclicCheckResult::InvalidStructure(err) => write!(f, "{err}"),
                CyclicCheckResult::WorkloadPartOfCycle(workload) => {
                    write!(f, "workload '{}' part of a cycle.", workload)
                }
            }
        }
    }

    pub fn dfs(state: &State) -> Result<(), CyclicCheckResult> {
        // stack is used to terminate the search properly
        let mut stack: VecDeque<&String> = VecDeque::new();

        // used to prevent visiting nodes repeatedly
        let mut visited: HashSet<&String> = HashSet::with_capacity(state.workloads.len());

        /* although the path container is used for lookups,
        measurements have shown that it is faster than associative data structure within this code path */
        let mut path: VecDeque<&String> = VecDeque::with_capacity(state.workloads.len());

        /* sort the map to have an constant equal outcome
        because the current data structure is randomly ordered because of HashMap's random seed */
        let mut data: Vec<&String> = state.workloads.keys().collect();
        data.sort();

        // iterate through all the nodes if the they are not already visited
        for workload_name in data {
            if visited.contains(workload_name) {
                continue;
            }

            log::debug!("searching for workload = '{}'", workload_name);
            stack.push_front(workload_name);
            while let Some(head) = stack.front() {
                let workload_spec = state.workloads.get(*head).ok_or_else(|| {
                    CyclicCheckResult::InvalidStructure(format!(
                        "workload '{head}' is not part of the state."
                    ))
                })?;

                if !visited.contains(head) {
                    log::debug!("visit '{}'", head);
                    visited.insert(head);
                    path.push_back(head);
                } else {
                    log::debug!("remove '{}' from path", head);
                    path.pop_back();
                    stack.pop_front();
                }

                // sort the map to have an constant equal outcome
                let mut dependencies: Vec<&String> = workload_spec.dependencies.keys().collect();
                dependencies.sort();

                for dependency in dependencies {
                    if !visited.contains(dependency) {
                        stack.push_front(dependency);
                    } else if path.contains(&dependency) {
                        log::debug!("workload '{dependency}' is part of a cycle.");
                        return Err(CyclicCheckResult::WorkloadPartOfCycle(
                            dependency.to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

pub type DeleteGraph = HashMap<String, HashMap<String, DeleteCondition>>;

pub struct ServerState {
    state: CompleteState,
    delete_conditions: DeleteGraph,
}

impl ServerState {
    pub fn new(state: CompleteState, delete_conditions: DeleteGraph) -> Self {
        ServerState {
            state,
            delete_conditions,
        }
    }

    pub fn has_cyclic_dependencies(&self) -> Result<(), CyclicCheckResult> {
        cyclic_check::dfs(&self.state.current_state)
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {

    use super::*;
    use common::{
        objects::AddCondition,
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };
    use std::{collections::HashSet, ops::Deref, time::Instant};

    const AGENT_NAME: &str = "agent_A";
    const RUNTIME: &str = "runtime X";
    const REQUEST_ID: &str = "request@id";
    const BENCHMARKING_NUMBER_OF_WORKLOADS: usize = 1000;

    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%20%20%20%20C%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning)
            .workload_dependency("C", "D", AddCondition::AddCondRunning)
            .workload_dependency("C", "A", AddCondition::AddCondRunning);

        let expected_nodes_part_of_a_cycle = ["A", "B", "C"];

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(matches!(
                result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.into_iter().any(|expected| w.contains(expected))
            ));
        }
    }

    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20C%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20E%20-%3E%20D%3B%0A%20%20%20%20D%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_2() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning)
            .workload_dependency("C", "F", AddCondition::AddCondRunning)
            .workload_dependency("F", "E", AddCondition::AddCondRunning)
            .workload_dependency("E", "D", AddCondition::AddCondRunning)
            .workload_dependency("D", "A", AddCondition::AddCondRunning);

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(matches!(
                result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(_))
            ));
        }
    }

    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_3() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "A", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["A", "B"];

        let mut actual = HashSet::new();
        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();

            assert!(matches!(
                &result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.contains(&w.replace("1_", "").deref())
            ));

            actual.insert(result.unwrap_err().to_string().replace("1_", ""));
        }

        assert_eq!(actual.len(), expected_nodes_part_of_a_cycle.len());
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_4() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "D", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["C", "H", "G", "F", "D"];

        let mut actual = HashSet::new();
        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();

            assert!(matches!(
                &result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.contains(&w.replace("1_", "").deref())
            ));

            actual.insert(result.unwrap_err().to_string().replace("1_", ""));
        }

        assert_eq!(actual.len(), expected_nodes_part_of_a_cycle.len());
    }

    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_5() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "A", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["A", "B", "D", "C", "H", "G", "F"];

        let mut actual = HashSet::new();
        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();

            assert!(matches!(
                &result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.contains(&w.replace("1_", "").deref())
            ));

            actual.insert(result.unwrap_err().to_string().replace("1_", ""));
        }

        assert_eq!(actual.len(), expected_nodes_part_of_a_cycle.len());
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20H%3B%0A%20%20%20%20A%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20A%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20C%20-%3E%20B%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20C%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20D%3B%0A%20%20%20%20D%20-%3E%20H%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_star_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "H", AddCondition::AddCondRunning)
            .workload_dependency("A", "D", AddCondition::AddCondRunning)
            .workload_dependency("B", "A", AddCondition::AddCondRunning)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["G", "D", "H", "D"];

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(matches!(
                result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.into_iter().any(|expected| w.contains(expected))
            ));
        }
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20H%3B%0A%20%20%20%20A%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20A%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20C%20-%3E%20B%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20C%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20D%3B%0A%20%20%20%20H%20-%3E%20D%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_star_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "H", AddCondition::AddCondRunning)
            .workload_dependency("A", "D", AddCondition::AddCondRunning)
            .workload_dependency("B", "A", AddCondition::AddCondRunning)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded);

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(result.is_ok());
        }
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20A%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20B%3B%0A%20%20%20%20G%20-%3E%20H%3B%0A%20%20%20%20H%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20F%3B%0A%7D
    #[test]
    fn utest_detect_self_cycle_in_dependencies_separated_graphs() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "A", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "F", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["F"]; // self cycle in one of the two graphs

        let mut actual = HashSet::new();
        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();

            assert!(matches!(
                &result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.contains(&w.replace("1_", "").deref())
            ));

            actual.insert(result.unwrap_err().to_string().replace("1_", ""));
        }

        assert_eq!(actual.len(), expected_nodes_part_of_a_cycle.len());
    }

    /// Graph visualized: 1) A -> A and 2) A -> B -> B
    #[test]
    fn utest_detect_self_cycle_in_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        // 1)
        let complete_state = CompleteStateBuilder::default()
            .with_workloads(&["A"])
            .workload_dependency("A", "A", AddCondition::AddCondRunning)
            .build();

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies();
        assert_eq!(
            result,
            Err(CyclicCheckResult::WorkloadPartOfCycle("A".to_string()))
        );

        // 2)
        let workloads = ["A", "B"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "B", AddCondition::AddCondRunning);

        let expected_nodes_part_of_a_cycle = ["B"]; // self cycle in one of the two graphs

        let mut actual = HashSet::new();
        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();

            assert!(matches!(
                &result,
                Err(CyclicCheckResult::WorkloadPartOfCycle(w)) if expected_nodes_part_of_a_cycle.contains(&w.replace("1_", "").deref())
            ));

            actual.insert(result.unwrap_err().to_string().replace("1_", ""));
        }

        assert_eq!(actual.len(), expected_nodes_part_of_a_cycle.len());
    }

    // Graph visualized: A -> B -> C (C is defined as dependency but missing in the state config)
    #[test]
    fn utest_detect_non_existing_workload_in_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning);

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();

            assert_eq!(
                result,
                Err(CyclicCheckResult::InvalidStructure(
                    "workload 'C' is not part of the state.".to_string()
                ))
            );
        }
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20F%3B%0A%20%20%20%20D%20-%3E%20G%3B%0A%20%20%20%20D%20-%3E%20H%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "D", AddCondition::AddCondRunning)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "H", AddCondition::AddCondSucceeded);

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(result.is_ok());
        }
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20B%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20F%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_2() {
        let _ = env_logger::builder().is_test(true).try_init();

        let builder = CompleteStateBuilder::default()
            .workload_spec("A")
            .workload_spec("B")
            .workload_spec("C")
            .workload_spec("D")
            .workload_spec("E")
            .workload_spec("F")
            .workload_spec("G")
            .workload_spec("H")
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "F", AddCondition::AddCondSucceeded);

        for start_node in ["A", "B", "C", "D", "E", "F", "G", "H"] {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(result.is_ok());
        }
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20A%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20B%3B%0A%20%20%20%20G%20-%3E%20H%3B%0A%20%20%20%20H%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20F%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_separated_graphs_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];
        let builder = CompleteStateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "A", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "F", AddCondition::AddCondSucceeded);

        for start_node in workloads {
            let builder = builder.clone();
            let complete_state = builder.set_start_node(start_node).build();
            let server_state = ServerState::new(complete_state, DeleteGraph::new());
            let result = server_state.has_cyclic_dependencies();
            assert!(result.is_ok());
        }
    }

    /// Graph visualized: 1000 Nodes, n_1 -> n_2 -> ... -> n_999 -> n_1
    #[test]
    fn utest_detect_cycle_in_dependencies_performance_1000_nodes() {
        let _ = env_logger::builder().is_test(true).try_init();
        use rand::{thread_rng, Rng};
        let root_name: String = thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(thread_rng().gen_range(10..30))
            .map(|x| x as char)
            .collect();

        let mut workload_root = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            root_name.clone(),
            RUNTIME.to_string(),
        );
        workload_root.dependencies.clear();

        let mut dependencies = vec![workload_root];
        for i in 1..BENCHMARKING_NUMBER_OF_WORKLOADS {
            let random_workload_name: String = thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(thread_rng().gen_range(10..30))
                .map(|x| x as char)
                .collect();
            let workload_name = format!("{}{}", random_workload_name, i); // concatenate with index to ensure unique name in collection
            let workload_i = generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                workload_name,
                RUNTIME.to_string(),
            );

            dependencies.last_mut().unwrap().dependencies =
                HashMap::from([(workload_i.name.clone(), AddCondition::AddCondRunning)]);
            dependencies.push(workload_i);
        }

        dependencies.last_mut().unwrap().dependencies =
            HashMap::from([(root_name.clone(), AddCondition::AddCondRunning)]);

        let mut complete_state = generate_test_complete_state(REQUEST_ID.to_string(), dependencies);
        complete_state.workload_states.clear();
        assert_eq!(
            complete_state.current_state.workloads.len(),
            BENCHMARKING_NUMBER_OF_WORKLOADS
        );

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let start = Instant::now();
        let result = server_state.has_cyclic_dependencies();
        let duration = start.elapsed();
        assert!(result.is_err());
        log::info!("{}", result.err().unwrap());
        log::info!(
            "time iterative cyclic dependency check: '{:?}' micro sek.",
            duration.as_micros()
        );
        assert!(duration.as_micros() < 5000);
    }

    #[derive(Clone)]
    struct CompleteStateBuilder(CompleteState);
    impl CompleteStateBuilder {
        fn default() -> Self {
            let mut complete_state =
                generate_test_complete_state(REQUEST_ID.to_string(), Vec::new());
            complete_state.workload_states.clear();
            CompleteStateBuilder(complete_state)
        }

        fn workload_spec(mut self, workload_name: &str) -> Self {
            let mut test_workload_spec = generate_test_workload_spec_with_param(
                AGENT_NAME.into(),
                workload_name.into(),
                RUNTIME.into(),
            );
            test_workload_spec.dependencies.clear();
            self.0
                .current_state
                .workloads
                .insert(workload_name.into(), test_workload_spec);
            self
        }

        fn with_workloads(mut self, workloads: &[&str]) -> Self {
            for w in workloads {
                let mut test_workload_spec = generate_test_workload_spec_with_param(
                    AGENT_NAME.into(),
                    w.to_string(),
                    RUNTIME.into(),
                );
                test_workload_spec.dependencies.clear();
                self.0
                    .current_state
                    .workloads
                    .insert(w.to_string(), test_workload_spec);
            }
            self
        }

        fn workload_dependency(
            mut self,
            workload: &str,
            depend_on: &str,
            add_condition: AddCondition,
        ) -> Self {
            self.0
                .current_state
                .workloads
                .get_mut(workload)
                .and_then(|w_spec| w_spec.dependencies.insert(depend_on.into(), add_condition));
            self
        }

        fn set_start_node(mut self, start_node: &str) -> Self {
            let new_name = format!("1_{start_node}");
            let entry = self.0.current_state.workloads.remove(start_node).unwrap();
            self.0
                .current_state
                .workloads
                .insert(new_name.clone(), entry);

            for workload_spec in self.0.current_state.workloads.values_mut() {
                if let Some(dep_condition) = workload_spec.dependencies.remove(start_node) {
                    workload_spec
                        .dependencies
                        .insert(new_name.clone(), dep_condition);
                }
            }
            self
        }

        fn build(self) -> CompleteState {
            self.0
        }
    }
}
