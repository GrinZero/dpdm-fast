use crate::parser::types::DependencyTree;
use std::collections::HashSet;

pub fn analyze_file(
    file: &str,
    dependency_tree: &DependencyTree,
    visited: &mut HashSet<String>,
    result: &mut Vec<String>,
) {
    if visited.contains(file) {
        return;
    }
    visited.insert(file.to_string());

    if let Some(arc_opt_deps) = dependency_tree.get(file) {
        if let Some(deps) = arc_opt_deps.as_ref().as_ref() {
            for dep in deps {
                if let Some(id) = &dep.id {
                    result.push(id.clone());
                    analyze_file(id, dependency_tree, visited, result);
                }
            }
        }
    }
}
