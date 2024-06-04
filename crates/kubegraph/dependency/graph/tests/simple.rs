use std::fmt;

use kubegraph_dependency_graph::{
    Graph, GraphPipeline, GraphPipelineClaim, GraphPipelineClaimOptions, Node,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Package<'a> {
    name: &'a str,
    provides: &'a [&'a str],
    requirements: &'a [&'a str],
}

impl<'a> Node for Package<'a> {
    type Key = &'a str;

    fn provided(&self) -> &[Self::Key] {
        self.provides
    }

    fn requirements(&self) -> &[Self::Key] {
        self.requirements
    }
}

impl<'a> fmt::Display for Package<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            name,
            provides,
            requirements,
        } = self;
        write!(f, "{name}")?;
        Ok(())
    }
}

#[test]
fn solve() {
    let mut graph = Graph::default();

    let node_a = Package {
        name: "A",
        provides: &["a"],
        requirements: &[],
    };
    let node_b = Package {
        name: "B",
        provides: &["b"],
        requirements: &["a"],
    };
    let node_c = Package {
        name: "C",
        provides: &["c"],
        requirements: &["b"],
    };
    let node_d = Package {
        name: "D",
        provides: &["d"],
        requirements: &["b"],
    };
    let node_e = Package {
        name: "E",
        provides: &["e"],
        requirements: &["b", "c", "d"],
    };
    let node_f = Package {
        name: "F",
        provides: &["c", "d", "e"],
        requirements: &["b"],
    };

    graph.add_node(node_a);
    graph.add_node(node_b);
    graph.add_node(node_c);
    graph.add_node(node_d);
    graph.add_node(node_e);
    graph.add_node(node_f);

    let claim = GraphPipelineClaim {
        option: GraphPipelineClaimOptions::default(),
        src: &["a"],
        sink: &["e"],
    };
    let pipelines = graph.build_pipeline(&claim).unwrap();

    let expected_pipelines = vec![GraphPipeline {
        nodes: vec![&node_b, &node_f],
    }];
    assert_eq!(pipelines, expected_pipelines);
}
