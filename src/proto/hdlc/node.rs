/*
    Copyright 2024 (C) Alexey Dynda

    This file is part of Tiny Protocol Library.

    GNU General Public License Usage

    Protocol Library is free software: you can redistribute it and/or modify
    it under the terms of the GNU Lesser General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    Protocol Library is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Lesser General Public License for more details.

    You should have received a copy of the GNU Lesser General Public License
    along with Protocol Library.  If not, see <http://www.gnu.org/licenses/>.

    Commercial License Usage

    Licensees holding valid commercial Tiny Protocol licenses may use this file in
    accordance with the commercial license agreement provided in accordance with
    the terms contained in a written agreement between you and Alexey Dynda.
    For further information contact via email on github account.
*/

use crate::proto::hdlc::frame::HdlcFrame;

/// Interface for a node in an HDLC processing pipeline.
///
/// Nodes can receive frames from upstream and send frames downstream.
pub trait NodeInterface {
    /// Register an upstream source node.
    fn add_source(&mut self, source: &Box<dyn NodeInterface>);
    /// Register a downstream sink node.
    fn add_sink(&mut self, sink: &Box<dyn NodeInterface>);
    /// Called when a frame is received from the RX direction.
    fn on_rx_frame(&mut self, frame: HdlcFrame);
    /// Called to transmit a frame in the TX direction.
    fn put_tx_frame(&mut self, frame: HdlcFrame);
}

/// A linear pipeline of [`NodeInterface`] nodes.
///
/// Nodes are chained so that each node's sink is the next node,
/// and each node's source is the previous node.
struct Pipeline {
    nodes: Vec<Box<dyn NodeInterface>>,
}

impl Pipeline {
    /// Create an empty pipeline.
    pub fn new() -> Pipeline {
        Pipeline {
            nodes: Vec::new(),
        }
    }

    /// Append a node to the pipeline, linking it to the previous node.
    pub fn add_node(&mut self, node: Box<dyn NodeInterface>) {
        self.nodes.push(node);
        if self.nodes.len() > 1 {
            let index = self.nodes.len() - 1;
            {
                let (left, right) = self.nodes.split_at_mut(index);
                let next = &right[0];
                left[index - 1].add_sink(next);
            }
            {
                let (left, right) = self.nodes.split_at_mut(index);
                let prev = &mut left[index - 1];
                right[0].add_source(prev);
            }
        }
    }

    /// Push an empty frame into the first node for RX processing.
    pub fn run_rx(&mut self) {
        self.nodes[0].on_rx_frame(HdlcFrame::new());
    }

    /// Push an empty frame into the last node for TX processing.
    pub fn run_tx(&mut self) {
        let index = self.nodes.len() - 1;
        self.nodes[index].put_tx_frame(HdlcFrame::new());
    }
}

#[cfg(test)]
mod unittest {
    use super::*;

    struct TestNode {
        rx_frames: Vec<HdlcFrame>,
        tx_frames: Vec<HdlcFrame>,
    }

    impl NodeInterface for TestNode {
        fn add_source(&mut self, _source: &Box<dyn NodeInterface>) {
        }

        fn add_sink(&mut self, _sink: &Box<dyn NodeInterface>) {
        }

        fn on_rx_frame(&mut self, frame: HdlcFrame) {
            self.rx_frames.push(frame);
        }

        fn put_tx_frame(&mut self, frame: HdlcFrame) {
            self.tx_frames.push(frame);
        }
    }

    impl TestNode {
        pub fn new() -> TestNode {
            TestNode {
                rx_frames: Vec::new(),
                tx_frames: Vec::new(),
            }
        }
    }
  
    #[test]
    fn test_single_node_pipeline() {
        let mut pipeline = Pipeline::new();
        let node = TestNode::new();
        pipeline.add_node(Box::new(node));
        pipeline.run_rx();
        // assert_eq!(pipeline.nodes[0].rx_frames.len(), 1);
        pipeline.run_tx();
        // assert_eq!(node_ref.tx_frames.len(), 1);
    }
 }