use std::rc::Rc;
use crate::{
    error::Error,
    grammar::clause::ClauseKind,
    iterators::{Pair, PairArena, PairNode, Pairs},
    table::ParseTable,
    RuleType,
};

enum Task {
    Visit(u32),
    FinishPair(u32),
}

pub(crate) fn build_pairs_from_table<'g, 'i, R: RuleType>(
    table: &ParseTable<'g, 'i, R>,
) -> Result<Pairs<'i, R>, Error<R>> {
    let start_clause = table.grammar.rule_entry[table.start_rule];
    let root_node = match table.memo.get(0, start_clause) {
        Some(node) => node,
        None => return Err(crate::error::diagnose_failure(table)),
    };

    let mut tasks = vec![Task::Visit(root_node)];
    let mut pair_nodes: Vec<PairNode<R>> = Vec::new();
    let mut child_idx = Vec::new();
    let mut child_stack: Vec<Vec<u32>> = vec![Vec::new()];
    let mut active_pairs: Vec<(u32, Option<u32>)> = Vec::new();

    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(node_idx) => {
                let match_node = &table.arena.nodes[node_idx as usize];
                let clause = &table.grammar.clauses[match_node.clause as usize];

                if clause.emit && clause.rule.is_some() {
                    let rule = (table.to_rule)(clause.rule.unwrap() as usize);
                    let start = match_node.start as usize;
                    let end = start + match_node.len as usize;
                    let prec_group = clause.prec_group;

                    let should_collapse = if let Some(&(parent_idx, parent_prec)) = active_pairs.last() {
                        let parent = &pair_nodes[parent_idx as usize];
                        parent.rule == rule
                            && parent.start == start
                            && parent.end == end
                            && prec_group.is_some()
                            && parent_prec == prec_group
                    } else {
                        false
                    };

                    if !should_collapse {
                        let pair_idx = pair_nodes.len() as u32;
                        pair_nodes.push(PairNode {
                            rule,
                            start,
                            end,
                            tag: clause.tag,
                            children: (0, 0),
                        });
                        active_pairs.push((pair_idx, prec_group));
                        child_stack.push(Vec::new());
                        tasks.push(Task::FinishPair(pair_idx));
                    }
                }

                // Collect children
                let mut children = Vec::new();
                if matches!(clause.kind, ClauseKind::OneOrMore { .. }) {
                    let mut cur = node_idx;
                    while matches!(
                        table.grammar.clauses[table.arena.nodes[cur as usize].clause as usize].kind,
                        ClauseKind::OneOrMore { .. }
                    ) {
                        let subs = table.arena.get_subs(cur);
                        if subs.is_empty() {
                            break;
                        }
                        children.push(subs[0]);
                        if subs.len() == 2 {
                            cur = subs[1];
                        } else if subs.len() >= 3 {
                            cur = subs[2];
                        } else {
                            break;
                        }
                    }
                } else {
                    let subs = table.arena.get_subs(node_idx);
                    children.extend_from_slice(subs);
                }

                // Push children in reverse order
                for &child in children.iter().rev() {
                    tasks.push(Task::Visit(child));
                }
            }
            Task::FinishPair(pair_idx) => {
                active_pairs.pop();
                let my_children = child_stack.pop().unwrap();
                let start = child_idx.len() as u32;
                let len = my_children.len() as u32;
                child_idx.extend(my_children);
                pair_nodes[pair_idx as usize].children = (start, start + len);
                child_stack.last_mut().unwrap().push(pair_idx);
            }
        }
    }

    let root_children = child_stack.pop().unwrap();
    let root_start = child_idx.len() as u32;
    let root_len = root_children.len() as u32;
    child_idx.extend(root_children);

    let arena = Rc::new(PairArena {
        nodes: pair_nodes,
        child_idx,
        tags: table.grammar.tags.clone(),
        input: table.input,
    });

    Ok(Pairs {
        arena,
        range: (root_start, root_start + root_len),
    })
}

pub(crate) fn build_pair_for_node<'g, 'i, R: RuleType>(
    table: &ParseTable<'g, 'i, R>,
    root_node: u32,
    default_rule: R,
) -> Pair<'i, R> {
    let mut tasks = vec![Task::Visit(root_node)];
    let mut pair_nodes: Vec<PairNode<R>> = Vec::new();
    let mut child_idx = Vec::new();
    let mut child_stack: Vec<Vec<u32>> = vec![Vec::new()];
    let mut active_pairs: Vec<(u32, Option<u32>)> = Vec::new();

    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(node_idx) => {
                let match_node = &table.arena.nodes[node_idx as usize];
                let clause = &table.grammar.clauses[match_node.clause as usize];

                let rule = if let Some(r) = clause.rule {
                    (table.to_rule)(r as usize)
                } else {
                    default_rule
                };
                let emit = clause.emit || node_idx == root_node;

                if emit {
                    let start = match_node.start as usize;
                    let end = start + match_node.len as usize;
                    let prec_group = clause.prec_group;

                    let should_collapse = if let Some(&(parent_idx, parent_prec)) = active_pairs.last() {
                        let parent = &pair_nodes[parent_idx as usize];
                        parent.rule == rule
                            && parent.start == start
                            && parent.end == end
                            && prec_group.is_some()
                            && parent_prec == prec_group
                    } else {
                        false
                    };

                    if !should_collapse {
                        let pair_idx = pair_nodes.len() as u32;
                        pair_nodes.push(PairNode {
                            rule,
                            start,
                            end,
                            tag: clause.tag,
                            children: (0, 0),
                        });
                        active_pairs.push((pair_idx, prec_group));
                        child_stack.push(Vec::new());
                        tasks.push(Task::FinishPair(pair_idx));
                    }
                }

                // Collect children
                let mut children = Vec::new();
                if matches!(clause.kind, ClauseKind::OneOrMore { .. }) {
                    let mut cur = node_idx;
                    while matches!(
                        table.grammar.clauses[table.arena.nodes[cur as usize].clause as usize].kind,
                        ClauseKind::OneOrMore { .. }
                    ) {
                        let subs = table.arena.get_subs(cur);
                        if subs.is_empty() {
                            break;
                        }
                        children.push(subs[0]);
                        if subs.len() == 2 {
                            cur = subs[1];
                        } else if subs.len() >= 3 {
                            cur = subs[2];
                        } else {
                            break;
                        }
                    }
                } else {
                    let subs = table.arena.get_subs(node_idx);
                    children.extend_from_slice(subs);
                }

                // Push children in reverse order
                for &child in children.iter().rev() {
                    tasks.push(Task::Visit(child));
                }
            }
            Task::FinishPair(pair_idx) => {
                active_pairs.pop();
                let my_children = child_stack.pop().unwrap();
                let start = child_idx.len() as u32;
                let len = my_children.len() as u32;
                child_idx.extend(my_children);
                pair_nodes[pair_idx as usize].children = (start, start + len);
                child_stack.last_mut().unwrap().push(pair_idx);
            }
        }
    }

    let root_children = child_stack.pop().unwrap();
    let root_node_idx = if !root_children.is_empty() {
        root_children[0]
    } else {
        let m = &table.arena.nodes[root_node as usize];
        let pair_idx = pair_nodes.len() as u32;
        pair_nodes.push(PairNode {
            rule: default_rule,
            start: m.start as usize,
            end: (m.start + m.len) as usize,
            tag: None,
            children: (0, 0),
        });
        pair_idx
    };

    let arena = Rc::new(PairArena {
        nodes: pair_nodes,
        child_idx,
        tags: table.grammar.tags.clone(),
        input: table.input,
    });

    Pair {
        arena,
        node: root_node_idx,
    }
}
