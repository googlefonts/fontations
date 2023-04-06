//! Support for generating graphviz files from our object graph

use super::{Graph, ObjectId, OffsetLen, Space};

pub struct GraphVizGraph<'a> {
    graph: &'a Graph,
    edges: Vec<GraphVizEdge>,
}

impl<'a> GraphVizGraph<'a> {
    pub(crate) fn from_graph(graph: &'a Graph) -> Self {
        let mut edges = Vec::new();
        for (parent_id, table) in &graph.objects {
            let parent = &graph.nodes[parent_id];
            for link in &table.offsets {
                let child = &graph.nodes[&link.object];
                let len = child.position - parent.position;
                edges.push(GraphVizEdge {
                    source: *parent_id,
                    target: link.object,
                    len,
                    type_: link.len,
                });
            }
        }
        GraphVizGraph { graph, edges }
    }

    /// Write out this graph as a graphviz file to the provided path.
    ///
    /// Overwrites any existing file at this location.
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let mut buf = Vec::new();
        dot2::render(self, &mut buf).unwrap();
        std::fs::write(path, &buf)
    }
}

#[derive(Clone, Debug)]
pub struct GraphVizEdge {
    source: ObjectId,
    target: ObjectId,
    len: u32,
    type_: OffsetLen,
}

impl<'a> dot2::GraphWalk<'a> for GraphVizGraph<'a> {
    type Node = ObjectId;

    type Edge = GraphVizEdge;

    type Subgraph = Space;

    fn nodes(&'a self) -> dot2::Nodes<'a, Self::Node> {
        self.graph.order.as_slice().into()
    }

    fn edges(&'a self) -> dot2::Edges<'a, Self::Edge> {
        self.edges.as_slice().into()
    }

    fn source(&'a self, edge: &Self::Edge) -> Self::Node {
        edge.source
    }

    fn target(&'a self, edge: &Self::Edge) -> Self::Node {
        edge.target
    }

    fn subgraphs(&'a self) -> dot2::Subgraphs<'a, Self::Subgraph> {
        let mut spaces: Vec<_> = self.graph.nodes.values().map(|x| x.space).collect();
        spaces.sort_unstable();
        spaces.dedup();
        spaces.into()
    }

    fn subgraph_nodes(&'a self, s: &Self::Subgraph) -> dot2::Nodes<'a, Self::Node> {
        self.graph
            .nodes
            .iter()
            .filter_map(|(id, node)| (node.space == *s).then_some(*id))
            .collect()
    }
}

impl<'a> dot2::Labeller<'a> for GraphVizGraph<'a> {
    type Node = ObjectId;

    type Edge = GraphVizEdge;

    type Subgraph = Space;

    fn graph_id(&'a self) -> dot2::Result<dot2::Id<'a>> {
        dot2::Id::new("TablePacking")
    }

    fn node_id(&'a self, n: &Self::Node) -> dot2::Result<dot2::Id<'a>> {
        dot2::Id::new(format!("N{}", n.0))
    }

    fn node_label<'b>(&'b self, n: &Self::Node) -> dot2::Result<dot2::label::Text<'b>> {
        let obj = &self.graph.objects[n];
        let node = &self.graph.nodes[n];

        let name = if obj.name.is_empty() {
            // if we have no name (generally because this is a test) then use
            // the object id instead.
            format!("{n:?} ({}B, S{})", obj.bytes.len(), node.space.0)
        } else {
            format!("{} ({}B, S{})", obj.name, obj.bytes.len(), node.space.0)
        };
        Ok(dot2::label::Text::LabelStr(name.into()))
    }

    fn edge_label(&'a self, e: &Self::Edge) -> dot2::label::Text<'a> {
        dot2::label::Text::LabelStr(e.len.to_string().into())
    }

    fn edge_color(&'a self, e: &Self::Edge) -> Option<dot2::label::Text<'a>> {
        if e.len > e.type_.max_value() {
            return Some(dot2::label::Text::LabelStr("firebrick".into()));
        }
        None
    }

    fn edge_style(&'a self, e: &Self::Edge) -> dot2::Style {
        if e.len > e.type_.max_value() {
            dot2::Style::Bold
        } else {
            dot2::Style::Solid
        }
    }
}
