use crate::model::{Skill, Workspace};

pub fn expand_env(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                if let Some(close) = chars[i + 2..].iter().position(|&ch| ch == '}') {
                    let name: String = chars[i + 2..i + 2 + close].iter().collect();
                    match std::env::var(&name) {
                        Ok(v) => out.push_str(&v),
                        Err(_) => {
                            out.push('$');
                            out.push('{');
                            out.push_str(&name);
                            out.push('}');
                        }
                    }
                    i += close + 3;
                    continue;
                }
            } else if chars[i + 1].is_ascii_alphanumeric() || chars[i + 1] == '_' {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                match std::env::var(&name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('$');
                        out.push_str(&name);
                    }
                }
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

pub fn expand_workspace(ws: &Workspace) -> Workspace {
    let mut ws = ws.clone();
    for skill in ws.skills.values_mut() {
        if let Skill::Mcp { command, args } = skill {
            *command = expand_env(command);
            for arg in args {
                *arg = expand_env(arg);
            }
        }
    }
    for harness in &mut ws.harnesses {
        if let Some(extra) = &mut harness.extra {
            *extra = expand_toml_value(extra);
        }
    }
    ws
}

fn expand_toml_value(value: &toml::Value) -> toml::Value {
    match value {
        toml::Value::String(s) => toml::Value::String(expand_env(s)),
        toml::Value::Array(items) => {
            toml::Value::Array(items.iter().map(expand_toml_value).collect())
        }
        toml::Value::Table(map) => toml::Value::Table(
            map.iter()
                .map(|(k, v)| (k.clone(), expand_toml_value(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_braced_env() {
        std::env::set_var("AGENTCTL_TEST_VAR", "hello");
        assert_eq!(expand_env("x ${AGENTCTL_TEST_VAR} y"), "x hello y");
    }

    #[test]
    fn expands_bare_env() {
        std::env::set_var("AGENTCTL_TEST_VAR", "hello");
        assert_eq!(expand_env("$AGENTCTL_TEST_VAR"), "hello");
    }

    #[test]
    fn leaves_unset_vars_untouched() {
        std::env::remove_var("AGENTCTL_TEST_UNSET");
        assert_eq!(expand_env("a $AGENTCTL_TEST_UNSET b"), "a $AGENTCTL_TEST_UNSET b");
        assert_eq!(expand_env("a ${AGENTCTL_TEST_UNSET} b"), "a ${AGENTCTL_TEST_UNSET} b");
    }

    #[test]
    fn leaves_non_env_dollar_untouched() {
        assert_eq!(expand_env("cost $5"), "cost $5");
    }
}