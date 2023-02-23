use k8s_openapi::api::core::v1::{
    Affinity, NodeAffinity, NodeSelector, NodeSelectorRequirement, NodeSelectorTerm,
    PreferredSchedulingTerm,
};

pub fn affinity() -> Affinity {
    Affinity {
        // KISS normal control plane nodes should be preferred
        node_affinity: Some(NodeAffinity {
            preferred_during_scheduling_ignored_during_execution: Some(vec![
                PreferredSchedulingTerm {
                    weight: 1,
                    preference: NodeSelectorTerm {
                        match_expressions: Some(vec![NodeSelectorRequirement {
                            key: "node-role.kubernetes.io/kiss-ephemeral-control-plane".into(),
                            operator: "DoesNotExist".into(),
                            values: None,
                        }]),
                        ..Default::default()
                    },
                },
            ]),
            required_during_scheduling_ignored_during_execution: Some(NodeSelector {
                node_selector_terms: vec![NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec!["ControlPlane".into()]),
                    }]),
                    ..Default::default()
                }],
            }),
        }),
        ..Default::default()
    }
}
