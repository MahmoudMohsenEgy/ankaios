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

use common::objects::{DeletedWorkload, FulfilledBy, WorkloadSpec};

#[cfg_attr(test, mockall_double::double)]
use crate::parameter_storage::ParameterStorage;

#[cfg(test)]
use mockall::automock;

pub struct DependencyStateValidator {}

#[cfg_attr(test, automock)]
impl DependencyStateValidator {
    pub fn create_fulfilled(workload: &WorkloadSpec, workload_state_db: &ParameterStorage) -> bool {
        workload
            .dependencies
            .iter()
            .all(|(dependency_name, add_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .map_or(false, |wl_state| add_condition.fulfilled_by(&wl_state))
            })
    }

    pub fn delete_fulfilled(
        workload: &DeletedWorkload,
        workload_state_db: &ParameterStorage,
    ) -> bool {
        workload
            .dependencies
            .iter()
            .all(|(dependency_name, delete_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .map_or(true, |wl_state| delete_condition.fulfilled_by(&wl_state))
            })
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
    use super::DependencyStateValidator;
    use common::{
        objects::{
            generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
            AddCondition, DeleteCondition, ExecutionState,
        },
        test_utils::generate_test_deleted_workload_with_dependencies,
    };
    use mockall::predicate;
    use std::collections::HashMap;

    use crate::parameter_storage::MockParameterStorage;

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const RUNTIME: &str = "runtime";

    #[test]
    fn utest_create_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .with(predicate::eq(WORKLOAD_NAME_2.to_owned()))
            .return_const(Some(ExecutionState::running()));

        assert!(DependencyStateValidator::create_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_create_fulfilled_no_dependencies() {
        let mut workload_with_dependencies = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        workload_with_dependencies.dependencies.clear(); // no inter-workload dependencies

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .never();

        assert!(DependencyStateValidator::create_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_create_fulfilled_no_workload_state_known() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        assert!(!DependencyStateValidator::create_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_create_fulfilled_unfulfilled_execution_state() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::succeeded()));

        assert!(!DependencyStateValidator::create_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_delete_fulfilled() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .with(predicate::eq(WORKLOAD_NAME_2.to_owned()))
            .return_const(Some(ExecutionState::succeeded()));

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_delete_fulfilled_unfulfilled_execution_state() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_delete_fulfilled_no_workload_state_known() {
        let deleted_workload_with_dependencies = generate_test_deleted_workload_with_dependencies(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            HashMap::from([(
                WORKLOAD_NAME_2.to_owned(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        assert!(DependencyStateValidator::delete_fulfilled(
            &deleted_workload_with_dependencies,
            &parameter_storage_mock
        ));
    }
}
