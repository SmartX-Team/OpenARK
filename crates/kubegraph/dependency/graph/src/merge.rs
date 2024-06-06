use std::{
    collections::{btree_map::Entry, BTreeMap},
    mem::swap,
};

pub trait NodeIndex {
    type Key: Ord;

    fn key(&self) -> Self::Key;
}

impl<T> NodeIndex for &T
where
    T: NodeIndex,
{
    type Key = <T as NodeIndex>::Key;

    fn key(&self) -> Self::Key {
        <T as NodeIndex>::key(*self)
    }
}

impl NodeIndex for &str {
    type Key = String;

    fn key(&self) -> Self::Key {
        (*self).into()
    }
}

impl NodeIndex for String {
    type Key = Self;

    fn key(&self) -> Self::Key {
        self.clone()
    }
}

pub trait GraphPipelineMerge<T> {
    fn merge_pipelines(self) -> Vec<Vec<GraphPipelineMergedNode<T>>>;
}

impl<A> GraphPipelineMerge<<<A as IntoIterator>::Item as IntoIterator>::Item> for A
where
    A: IntoIterator,
    <A as IntoIterator>::Item: IntoIterator,
    <<A as IntoIterator>::Item as IntoIterator>::IntoIter: DoubleEndedIterator,
    <<A as IntoIterator>::Item as IntoIterator>::Item: NodeIndex,
{
    fn merge_pipelines(
        self,
    ) -> Vec<Vec<GraphPipelineMergedNode<<<A as IntoIterator>::Item as IntoIterator>::Item>>> {
        let mut map = ReversedNode::default();
        for nodes in self.into_iter() {
            let mut map = &mut map;
            let mut nodes = nodes.into_iter().rev().peekable();
            while let Some(node) = nodes.next() {
                let is_first = nodes.peek().is_none();
                map = map.resolve(node, is_first);
            }
        }

        map.into_disaggregated()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GraphPipelineMergedNode<T> {
    Item(Vec<T>),
    Next(usize),
}

struct ReversedNode<T>
where
    T: NodeIndex,
{
    neighbors: Vec<T>,
    prevs: BTreeMap<<T as NodeIndex>::Key, Self>,
}

impl<T> Default for ReversedNode<T>
where
    T: NodeIndex,
{
    fn default() -> Self {
        Self {
            neighbors: Vec::default(),
            prevs: BTreeMap::default(),
        }
    }
}

impl<T> ReversedNode<T>
where
    T: NodeIndex,
{
    fn new(node: T) -> Self {
        Self {
            neighbors: vec![node],
            prevs: BTreeMap::default(),
        }
    }

    fn resolve(&mut self, node: T, is_first: bool) -> &mut Self {
        match self.prevs.entry(node.key()) {
            Entry::Vacant(entry) => entry.insert(Self::new(node)),
            Entry::Occupied(entry) => {
                let entry = entry.into_mut();
                if is_first {
                    entry.neighbors.push(node);
                }
                entry
            }
        }
    }

    fn into_disaggregated(self) -> Vec<Vec<GraphPipelineMergedNode<T>>> {
        let Self {
            neighbors: _,
            prevs,
        } = self;

        let mut pipelines = Vec::default();
        let mut work_stack: Vec<_> = prevs
            .into_values()
            .map(|node| ReversedNodeCursor { depth: 0, node })
            .collect();

        let mut stack = Vec::default();
        while let Some(ReversedNodeCursor {
            depth,
            node: ReversedNode { neighbors, prevs },
        }) = work_stack.pop()
        {
            stack.truncate(depth);
            stack.push(GraphPipelineMergedNode::Item(neighbors));

            if prevs.len() != 1 {
                let mut nodes = Vec::default();
                for src in stack.iter_mut().rev() {
                    match src {
                        GraphPipelineMergedNode::Item(_) => {
                            let mut node = GraphPipelineMergedNode::Next(pipelines.len());
                            swap(src, &mut node);
                            nodes.push(node);
                        }
                        GraphPipelineMergedNode::Next(index) => {
                            nodes.push(GraphPipelineMergedNode::Next(*index));
                            nodes.shrink_to_fit();
                            break;
                        }
                    }
                }
                pipelines.push(nodes);
            }

            for node in prevs.into_values() {
                let cursor = ReversedNodeCursor {
                    depth: stack.len(),
                    node,
                };
                work_stack.push(cursor);
            }
        }
        pipelines
    }
}

struct ReversedNodeCursor<T>
where
    T: NodeIndex,
{
    depth: usize,
    node: ReversedNode<T>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let pipelines = vec![vec!["a", "b", "c", "d"], vec!["x", "y", "z", "c", "d"]];

        let merged_pipelines = pipelines.merge_pipelines();
        let expected_pipelines = vec![
            vec![
                GraphPipelineMergedNode::Item(vec!["c"]),
                GraphPipelineMergedNode::Item(vec!["d"]),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["x"]),
                GraphPipelineMergedNode::Item(vec!["y"]),
                GraphPipelineMergedNode::Item(vec!["z"]),
                GraphPipelineMergedNode::Next(0),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["a"]),
                GraphPipelineMergedNode::Item(vec!["b"]),
                GraphPipelineMergedNode::Next(0),
            ],
        ];
        assert_eq!(merged_pipelines, expected_pipelines);
    }

    #[test]
    fn simple_duplicated_depth_0() {
        let pipelines = vec![
            vec!["a", "b", "c", "d"],
            vec!["x", "y", "z", "c", "d"],
            vec!["x", "y", "z", "c", "d"],
        ];

        let merged_pipelines = pipelines.merge_pipelines();
        let expected_pipelines = vec![
            vec![
                GraphPipelineMergedNode::Item(vec!["c"]),
                GraphPipelineMergedNode::Item(vec!["d"]),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["x", "x"]),
                GraphPipelineMergedNode::Item(vec!["y"]),
                GraphPipelineMergedNode::Item(vec!["z"]),
                GraphPipelineMergedNode::Next(0),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["a"]),
                GraphPipelineMergedNode::Item(vec!["b"]),
                GraphPipelineMergedNode::Next(0),
            ],
        ];
        assert_eq!(merged_pipelines, expected_pipelines);
    }

    #[test]
    fn simple_duplicated_depth_1() {
        let pipelines = vec![
            vec!["a", "b", "c", "d"],
            vec!["x", "y", "z", "c", "d"],
            vec!["y", "z", "c", "d"],
        ];

        let merged_pipelines = pipelines.merge_pipelines();
        let expected_pipelines = vec![
            vec![
                GraphPipelineMergedNode::Item(vec!["c"]),
                GraphPipelineMergedNode::Item(vec!["d"]),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["x"]),
                GraphPipelineMergedNode::Item(vec!["y", "y"]),
                GraphPipelineMergedNode::Item(vec!["z"]),
                GraphPipelineMergedNode::Next(0),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["a"]),
                GraphPipelineMergedNode::Item(vec!["b"]),
                GraphPipelineMergedNode::Next(0),
            ],
        ];
        assert_eq!(merged_pipelines, expected_pipelines);
    }

    #[test]
    fn simple_unduplicated() {
        let pipelines = vec![
            vec!["a", "b", "c", "d"],
            vec!["x", "y", "z", "c", "d"],
            vec!["y", "z", "c", "d", "e"],
        ];

        let merged_pipelines = pipelines.merge_pipelines();
        let expected_pipelines = vec![
            vec![
                GraphPipelineMergedNode::Item(vec!["y"]),
                GraphPipelineMergedNode::Item(vec!["z"]),
                GraphPipelineMergedNode::Item(vec!["c"]),
                GraphPipelineMergedNode::Item(vec!["d"]),
                GraphPipelineMergedNode::Item(vec!["e"]),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["c"]),
                GraphPipelineMergedNode::Item(vec!["d"]),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["x"]),
                GraphPipelineMergedNode::Item(vec!["y"]),
                GraphPipelineMergedNode::Item(vec!["z"]),
                GraphPipelineMergedNode::Next(1),
            ],
            vec![
                GraphPipelineMergedNode::Item(vec!["a"]),
                GraphPipelineMergedNode::Item(vec!["b"]),
                GraphPipelineMergedNode::Next(1),
            ],
        ];
        assert_eq!(merged_pipelines, expected_pipelines);
    }
}
