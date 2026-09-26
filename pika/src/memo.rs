pub(crate) type NodeIdx = u32;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct MatchNode {
    pub(crate) clause: u32,
    pub(crate) start: u32,
    pub(crate) len: u32,
    pub(crate) first_sub: u32,
    pub(crate) subs_start: u32,
    pub(crate) subs_len: u32,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct Arena {
    pub(crate) nodes: Vec<MatchNode>,
    pub(crate) subs: Vec<NodeIdx>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(
        &mut self,
        clause: u32,
        start: u32,
        len: u32,
        first_sub: u32,
        sub_nodes: &[NodeIdx],
    ) -> NodeIdx {
        let subs_start = self.subs.len() as u32;
        let subs_len = sub_nodes.len() as u32;
        self.subs.extend_from_slice(sub_nodes);
        let node_idx = self.nodes.len() as u32;
        self.nodes.push(MatchNode {
            clause,
            start,
            len,
            first_sub,
            subs_start,
            subs_len,
        });
        node_idx
    }

    #[inline]
    #[allow(dead_code)]
    pub fn get_subs(&self, node_idx: NodeIdx) -> &[NodeIdx] {
        let node = &self.nodes[node_idx as usize];
        let start = node.subs_start as usize;
        let end = start + node.subs_len as usize;
        &self.subs[start..end]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MemoTable {
    pub(crate) cols: Vec<Vec<(u32, NodeIdx)>>,
}

impl MemoTable {
    pub fn new(input_len: usize) -> Self {
        Self {
            cols: vec![Vec::new(); input_len + 1],
        }
    }

    #[inline]
    pub fn get(&self, pos: usize, clause: u32) -> Option<NodeIdx> {
        if pos >= self.cols.len() {
            return None;
        }
        let col = &self.cols[pos];
        col.binary_search_by_key(&clause, |&(c, _)| c)
            .ok()
            .map(|i| col[i].1)
    }

    #[inline]
    pub fn put(&mut self, pos: usize, clause: u32, node: NodeIdx) {
        let col = &mut self.cols[pos];
        match col.binary_search_by_key(&clause, |&(c, _)| c) {
            Ok(i) => col[i].1 = node,
            Err(i) => col.insert(i, (clause, node)),
        }
    }
}
