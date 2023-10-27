use std::fmt::Display;

use crate::objects::workload_execution_instance_name::INSTANCE_NAME_SEPARATOR;

#[derive(Debug, Eq, PartialEq)]
pub struct AgentName(String);

impl AgentName {
    pub fn get(&self) -> &str {
        &self.0
    }

    // [impl->swdd~agent-existing-workloads-finds-list~1]
    pub fn get_filter_regex(&self) -> String {
        format!("[{}]{}$", INSTANCE_NAME_SEPARATOR, self.0)
    }

    // [impl->swdd~agent-existing-workloads-finds-list~1]
    pub fn get_filter_suffix(&self) -> String {
        format!("{}{}", INSTANCE_NAME_SEPARATOR, self.0)
    }
}

impl From<String> for AgentName {
    fn from(value: String) -> Self {
        AgentName(value)
    }
}

impl From<&str> for AgentName {
    fn from(value: &str) -> Self {
        AgentName::from(value.to_string())
    }
}

impl Display for AgentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
    use super::AgentName;

    const AGENT_NAME: &str = "agent";

    // [utest->swdd~agent-existing-workloads-finds-list~1]
    #[test]
    fn utest_agent_name_get_filter_regex() {
        assert_eq!(
            format!("[.]{AGENT_NAME}$"),
            AgentName::from(AGENT_NAME).get_filter_regex()
        );
    }

    // [utest->swdd~agent-existing-workloads-finds-list~1]
    #[test]
    fn utest_agent_name_get_filter_suffix() {
        assert_eq!(
            format!(".{AGENT_NAME}"),
            AgentName::from(AGENT_NAME).get_filter_suffix()
        );
    }
}
