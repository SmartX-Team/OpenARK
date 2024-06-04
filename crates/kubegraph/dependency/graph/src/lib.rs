use std::{
    collections::{BTreeSet, VecDeque},
    fmt,
};

#[derive(Clone, Debug)]
pub struct Graph<N> {
    nodes: Vec<N>,
}

impl<N> Default for Graph<N> {
    fn default() -> Self {
        Self {
            nodes: Vec::default(),
        }
    }
}

impl<N> Graph<N>
where
    N: Node,
{
    pub fn add_node(&mut self, node: N) {
        self.nodes.push(node)
    }

    pub fn build_pipeline(
        &self,
        claim: &GraphPipelineClaim<<N as Node>::Key>,
    ) -> Option<Vec<GraphPipeline<N>>> {
        let GraphPipelineClaim {
            option: GraphPipelineClaimOptions { fastest, max_depth },
            src: claim_src,
            sink: claim_sink,
        } = claim;

        // Prepare initial nodes to trigger the building
        let mut states: VecDeque<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, sink)| claim_src.contains_all(sink.requirements()))
            .map(|(sink_index, sink)| GraphVisitState {
                keys: claim_src.iter().chain(sink.provided()).collect(),
                node: sink,
                travelled: vec![sink_index],
            })
            .collect();

        let mut pipelines = vec![];
        while let Some(GraphVisitState {
            keys: provided,
            node: src,
            travelled,
        }) = states.pop_front()
        {
            for (sink_index, sink) in self.nodes.iter().enumerate() {
                // Test the pre-constraints
                if travelled.contains(&sink_index) || !provided.contains_all(sink.requirements()) {
                    continue;
                }

                // Register the output pipelines
                let mut provided = provided.clone();
                provided.extend(sink.provided());
                if provided.contains_all(claim_sink) {
                    let pipeline = GraphPipeline {
                        nodes: travelled
                            .iter()
                            .copied()
                            .map(|index| &self.nodes[index])
                            .chain(Some(sink))
                            .collect(),
                    };

                    if *fastest {
                        return Some(vec![pipeline]);
                    } else {
                        pipelines.push(pipeline);
                        continue;
                    }
                }

                // Test the post-constraints
                if let Some(max_depth) = *max_depth {
                    if max_depth <= travelled.len() + 1 {
                        continue;
                    }
                }

                // Mark the sink node as visited
                let next = GraphVisitState {
                    keys: provided,
                    node: sink,
                    travelled: {
                        let mut travelled = travelled.clone();
                        travelled.push(sink_index);
                        travelled
                    },
                };
                states.push_back(next)
            }
        }

        if pipelines.is_empty() {
            None
        } else {
            Some(pipelines)
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GraphPipelineClaim<'a, T> {
    pub option: GraphPipelineClaimOptions,
    pub src: &'a [T],
    pub sink: &'a [T],
}

#[derive(Copy, Clone, Debug)]
pub struct GraphPipelineClaimOptions {
    pub fastest: bool,
    pub max_depth: Option<usize>,
}

impl Default for GraphPipelineClaimOptions {
    fn default() -> Self {
        Self {
            fastest: true,
            max_depth: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphPipeline<'a, N> {
    pub nodes: Vec<&'a N>,
}

impl<'a, N> fmt::Display for GraphPipeline<'a, N>
where
    N: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut multiple_nodes = false;

        let Self { nodes } = self;
        for node in nodes {
            if multiple_nodes {
                " -> ".fmt(f)?;
            } else {
                multiple_nodes = true;
            }

            node.fmt(f)?;
        }
        Ok(())
    }
}

struct GraphVisitState<'a, N>
where
    N: Node,
{
    keys: BTreeSet<&'a <N as Node>::Key>,
    node: &'a N,
    travelled: Vec<usize>,
}

pub trait Node
where
    Self: fmt::Debug,
{
    type Key: Clone + fmt::Debug + Ord + ToString;

    fn provided(&self) -> &[Self::Key];

    fn requirements(&self) -> &[Self::Key];
}

trait ContainsAll<T>
where
    Self: Contains<T>,
{
    fn contains_all(&self, required: &[T]) -> bool {
        required.iter().all(|item| self.contains(item))
    }
}

impl<T, Item> ContainsAll<T> for Item where Item: ?Sized + Contains<T> {}

trait Contains<T> {
    fn contains(&self, required: &T) -> bool;
}

impl<T> Contains<T> for [T]
where
    T: PartialEq,
{
    fn contains(&self, required: &T) -> bool {
        <[T]>::contains(self, required)
    }
}

impl<T> Contains<T> for BTreeSet<T>
where
    T: Ord,
{
    fn contains(&self, required: &T) -> bool {
        BTreeSet::contains(self, required)
    }
}

impl<T> Contains<T> for BTreeSet<&T>
where
    T: Ord,
{
    fn contains(&self, required: &T) -> bool {
        BTreeSet::contains(self, required)
    }
}
