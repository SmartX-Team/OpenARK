pub mod merge;

use std::{
    collections::{BTreeSet, VecDeque},
    fmt,
    slice::Iter,
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

impl<N> FromIterator<N> for Graph<N> {
    fn from_iter<T: IntoIterator<Item = N>>(iter: T) -> Self {
        Self {
            nodes: iter.into_iter().collect(),
        }
    }
}

impl<N> fmt::Display for Graph<N>
where
    N: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for node in &self.nodes {
            writeln!(f, "{node}")?;
        }
        Ok(())
    }
}

impl<N> IntoIterator for Graph<N> {
    type Item = <Vec<N> as IntoIterator>::Item;

    type IntoIter = <Vec<N> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        let Self { nodes } = self;
        nodes.into_iter()
    }
}

impl<'a, N> IntoIterator for &'a Graph<N> {
    type Item = &'a N;

    type IntoIter = Iter<'a, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<N> Graph<N> {
    pub fn add_node(&mut self, node: N) {
        self.nodes.push(node)
    }

    pub fn iter(&self) -> Iter<'_, N> {
        self.nodes.iter()
    }
}

impl<N> Graph<N>
where
    N: Node,
{
    pub fn build_pipeline(
        &self,
        claim: &GraphPipelineClaim<<N as Node>::Feature>,
    ) -> Option<Vec<GraphPipeline<N>>> {
        let GraphPipelineClaim {
            option: GraphPipelineClaimOptions { fastest, max_depth },
            src: claim_src,
            sink: claim_sink,
        } = claim;

        if claim_sink.is_empty() || claim_src.contains_all(claim_sink) {
            return Some(vec![]);
        }

        // Prepare initial nodes to trigger the building
        let mut pipelines = Vec::default();
        let mut states = VecDeque::default();
        for (sink_index, sink) in self.nodes.iter().enumerate() {
            // Test the pre-constraints
            if !claim_src.contains_all(sink.requirements()) {
                continue;
            }

            // Register the output pipelines
            let mut provided: BTreeSet<_> = claim_src.iter().collect();
            provided.extend(sink.provided());
            if provided.contains_all(&claim_sink) {
                let pipeline = GraphPipeline { nodes: vec![sink] };

                if *fastest {
                    return Some(vec![pipeline]);
                } else {
                    pipelines.push(pipeline);
                    continue;
                }
            }

            // Mark the sink node as visited
            states.push_back(GraphVisitState {
                features: claim_src.iter().chain(sink.provided()).collect(),
                travelled: vec![sink_index],
            });
        }

        while let Some(GraphVisitState {
            features: provided,
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
                if sink.is_final() {
                    continue;
                }

                // Mark the sink node as visited
                let next = GraphVisitState {
                    features: provided,
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

struct GraphVisitState<'a, T> {
    features: BTreeSet<&'a T>,
    travelled: Vec<usize>,
}

pub trait Node {
    type Feature: Ord;

    fn is_final(&self) -> bool {
        false
    }

    fn provided(&self) -> &[Self::Feature];

    fn requirements(&self) -> &[Self::Feature];
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
