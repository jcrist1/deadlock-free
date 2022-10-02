use std::{
    marker::PhantomData,
    sync::mpsc::{Receiver, RecvError, SendError, Sender},
    thread::{self, JoinHandle},
};

use frunk::{hlist, HCons, HList, HNil};

use crate::{
    hierarchical_state::{False, Filter, SharedState, True},
    util::{SafeType, SharedMutex},
};

/// Thoughts
/// input can be unsynchronised => then it's a wrapper for an enum
/// input can be synchronised => then it's a tuple
/// output can be broadcast then we need to clone
/// output can be partitioned
///
///
///
pub struct Subscription<Subscribed, Subscription> {
    subscribed: Subscribed,
    subscription: Subscription,
}

pub trait Runnable {
    type JoinType;
    fn run(self) -> Self::JoinType;
}

impl Runnable for HNil {
    type JoinType = HNil;
    fn run(self) -> Self::JoinType {
        HNil
    }
}

impl<
        I: SafeType,
        Inputs: Receive<I> + 'static,
        S: SafeType + Clone,
        O: SafeType,
        Outputs: Send + 'static + Broadcast<O> + Send,
        F: for<'a> Fn(I, &'a S) -> O + 'static + Send,
        Tail: Runnable,
    > Runnable for HCons<Node<I, Inputs, S, O, Outputs, F>, Tail>
{
    type JoinType = HCons<JoinHandle<Result<(), SendError<O>>>, Tail::JoinType>;
    fn run(self) -> HCons<JoinHandle<Result<(), SendError<O>>>, Tail::JoinType> {
        // todo: head.run()
        HCons {
            head: self.head.run(),
            tail: self.tail.run(),
        }
    }
}

trait NodeHListOps {
    fn add_node<
        FilterType: Filter,
        S: SafeType,
        O: SafeType,
        F: for<'a> Fn(Self::SubscriptionOutput, &'a S) -> O + 'static + Send,
    >(
        self,
        f: F,
        state: S,
    ) -> HCons<Node<Self::SubscriptionOutput, Self::Subscription, S, O, HNil, F>, Self::Subscribed>
    where
        Self: Subscribable<FilterType>,
    {
        Subscribable::<FilterType>::subscribe_node(self, f, state)
    }
}

impl<T: NodeHList> NodeHListOps for T {}

trait Subscribable<FilterType: Filter>
where
    Self: Sized,
{
    type Subscribed;
    type SubscriptionOutput;
    type Subscription: Receive<Self::SubscriptionOutput>;

    fn subscribe(self) -> Subscription<Self::Subscribed, Self::Subscription>;

    fn subscribe_node<
        O,
        S,
        F: for<'a> Fn(Self::SubscriptionOutput, &'a S) -> O + 'static + Send,
    >(
        self,
        f: F,
        state: S,
    ) -> HCons<Node<Self::SubscriptionOutput, Self::Subscription, S, O, HNil, F>, Self::Subscribed>
    {
        let Subscription {
            subscribed,
            subscription,
        } = self.subscribe();
        let new_node = Node {
            computation: f,
            inputs: subscription,
            _input: PhantomData,
            state,
            _output: PhantomData,
            outputs: HNil,
        };
        HCons {
            head: new_node,
            tail: subscribed,
        }
    }
}

impl Subscribable<HNil> for HNil {
    type Subscribed = HNil;

    type Subscription = HNil;

    fn subscribe(self) -> Subscription<Self::Subscribed, Self::Subscription> {
        Subscription {
            subscribed: HNil,
            subscription: HNil,
        }
    }

    type SubscriptionOutput = HNil;
}

trait NodeHList {}
impl<
        I: SafeType,
        Inputs: Receive<I> + Send + 'static,
        S: SafeType + Clone,
        O: SafeType,
        Outputs: Broadcast<O> + Send + 'static,
        F: for<'a> Fn(I, &'a S) -> O + Send + 'static,
        Tail: Filter,
        TailOutput: SafeType,
        TailReceiver: frunk::hlist::HList + Receive<TailOutput> + 'static,
        NodeTail: NodeHList
            + Subscribable<Tail, Subscription = TailReceiver, SubscriptionOutput = TailOutput>,
    > Subscribable<HCons<True, Tail>> for HCons<Node<I, Inputs, S, O, Outputs, F>, NodeTail>
{
    type Subscribed =
        HCons<Node<I, Inputs, S, O, HCons<Sender<O>, Outputs>, F>, NodeTail::Subscribed>;
    type SubscriptionOutput = HCons<O, NodeTail::SubscriptionOutput>;
    type Subscription = HCons<Receiver<O>, NodeTail::Subscription>;

    fn subscribe(self) -> Subscription<Self::Subscribed, Self::Subscription> {
        let HCons { head, tail } = self;
        let (new_head, new_recv) = head.add_subscription_channel();
        let Subscription {
            subscribed: tail_subscribed,
            subscription: tail_subscription,
        } = tail.subscribe();
        Subscription {
            subscribed: HCons {
                head: new_head,
                tail: tail_subscribed,
            },
            subscription: HCons {
                head: new_recv,
                tail: tail_subscription,
            },
        }
    }
}

impl<
        I: SafeType,
        Inputs: Send + 'static,
        S: SafeType,
        O: SafeType,
        Outputs,
        F: for<'a> Fn(I, &'a S) -> O + Send + 'static,
        Tail: Filter,
        NodeTail: NodeHList + Subscribable<Tail>,
    > Subscribable<HCons<False, Tail>> for HCons<Node<I, Inputs, S, O, Outputs, F>, NodeTail>
{
    type Subscribed = HCons<Node<I, Inputs, S, O, Outputs, F>, NodeTail::Subscribed>;
    type SubscriptionOutput = NodeTail::SubscriptionOutput;
    type Subscription = NodeTail::Subscription;

    fn subscribe(self) -> Subscription<Self::Subscribed, Self::Subscription> {
        let HCons { head, tail } = self;
        let Subscription {
            subscribed: tail_subscribed,
            subscription: tail_subscription,
        } = tail.subscribe();
        Subscription {
            subscribed: HCons {
                head,
                tail: tail_subscribed,
            },
            subscription: tail_subscription,
        }
    }
}

impl NodeHList for HNil {}

impl<H, T: NodeHList> NodeHList for HCons<H, T> {}

trait DepList {}

impl DepList for HNil {}

impl<H: NodeHList, T: DepList> DepList for HCons<H, T> {}

struct Subscribers<Output, Outputs> {
    _output: PhantomData<Output>,
    outputs: Outputs,
}
pub trait Receive<T>: Send {
    fn receive(&self) -> Result<T, RecvError>;
}

impl Receive<HNil> for HNil {
    fn receive(&self) -> Result<HNil, RecvError> {
        Ok(HNil)
    }
}

impl<
        H: SafeType,
        Tail: SafeType,
        RecvTail: Receive<Tail> + frunk::hlist::HList + Send + 'static,
    > Receive<HCons<H, Tail>> for HCons<Receiver<H>, RecvTail>
{
    fn receive(&self) -> Result<HCons<H, Tail>, RecvError> {
        let HCons { head, tail } = self;
        let head = head.recv()?;
        let tail = tail.receive()?;
        Ok(HCons { head, tail })
    }
}

impl<T: Send> Receive<T> for Receiver<T> {
    fn receive(&self) -> Result<T, RecvError> {
        self.recv()
    }
}

trait Broadcast<T>: Send {
    fn broadcast(&self, t: T) -> Result<(), SendError<T>>;
}

impl<T: SafeType> Broadcast<T> for HNil {
    fn broadcast(&self, _t: T) -> Result<(), SendError<T>> {
        Ok(())
    }
}

impl<T: SafeType, Tail: Broadcast<T>> Broadcast<T> for HCons<Sender<T>, Tail> {
    fn broadcast(&self, t: T) -> Result<(), SendError<T>> {
        let HCons { head, tail } = self;
        head.send(t.clone())?;
        tail.broadcast(t)
    }
}

struct Node<FlowInput, Inputs, State, FlowOutput, Outputs, F> {
    _input: PhantomData<FlowInput>,
    inputs: Inputs,
    state: State,
    _output: PhantomData<FlowOutput>,
    outputs: Outputs,
    computation: F,
}

impl<I, S, O, F: for<'a> Fn(I, &'a S) -> O> Node<I, Receiver<I>, S, O, HNil, F> {
    fn new(computation: F, input: Receiver<I>, state: S) -> Self {
        Node {
            computation,
            inputs: input,
            _input: PhantomData,
            state,
            _output: PhantomData,
            outputs: HNil,
        }
    }
}

impl<
        I: SafeType,
        Inputs: Receive<I> + Send + 'static,
        S: SafeType + Clone,
        O: SafeType,
        Outputs: Broadcast<O> + 'static,
        F: for<'a> Fn(I, &'a S) -> O + Send + 'static,
    > Node<I, Inputs, S, O, Outputs, F>
{
    fn add_subscription_channel(
        self,
    ) -> (
        Node<I, Inputs, S, O, HCons<Sender<O>, Outputs>, F>,
        Receiver<O>,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let Node {
            inputs,
            _input,
            state,
            _output,
            outputs,
            computation,
        } = self;
        (
            Node {
                inputs,
                _input,
                state,
                _output,
                outputs: HCons {
                    head: sender,
                    tail: outputs,
                },
                computation,
            },
            receiver,
        )
    }

    pub fn subscribe_node<O2: SafeType, F2: for<'a> Fn(O, &'a S) -> O2>(
        self,
        new_computation: F2,
    ) -> (
        Node<I, Inputs, S, O, HCons<Sender<O>, Outputs>, F>,
        Node<O, HList!(Receiver<O>), S, O2, HNil, F2>,
    ) {
        let (subscribed_node, receiver_for_new_node) = self.add_subscription_channel();
        let new_node = Node {
            inputs: hlist!(receiver_for_new_node),
            state: subscribed_node.state.clone(),
            _output: PhantomData,
            outputs: HNil,
            computation: new_computation,
            _input: PhantomData,
        };
        (subscribed_node, new_node)
    }

    fn run(self) -> JoinHandle<Result<(), SendError<O>>> {
        let Self {
            inputs: input,
            state,
            _output,
            outputs,
            computation,
            ..
        } = self;
        thread::spawn(move || -> Result<(), SendError<O>> {
            while let Ok(i) = input.receive() {
                let output = computation(i, &state);
                outputs.broadcast(output)?;
            }
            Ok(())
        })
    }
}

///  flow in
///  ___
///     \
///  ____\___
///      /
///  ___/
///
///      ___
///     /    
/// ___/____
///    \    
///     \___

struct Dag<State, Nodes: NodeHList, Dependency> {
    nodes: Nodes,
    state: State,
    dependency: Dependency,
}

impl Dag<HNil, HNil, HNil> {
    fn new() -> Self {
        Dag {
            nodes: HNil,
            state: HNil,
            dependency: HNil,
        }
    }
}

#[cfg(test)]
mod test {
    use super::Receive;
    use std::{
        sync::{Arc, Mutex},
        thread,
    };

    #[test]
    fn test_deadlock() {
        let input = Arc::new(Mutex::new((0..100).collect::<Vec<_>>()));
        let input_clone = input.clone();
        let output = Arc::new(Mutex::new(vec![100]));
        let output_clone = output.clone();
        let join_1 = thread::spawn(move || loop {
            if let Some(i) = input_clone.lock().unwrap().pop() {
                output_clone.lock().unwrap().push(i);
            } else {
                break;
            }
        });

        //        let join_2 = thread::spawn(move || loop {
        //            if let Some(i) = output.lock().unwrap().pop() {
        //                input.lock().unwrap().push(i);
        //            } else {
        //                break;
        //            }
        //        });

        join_1.join().unwrap();
        //join_2.join();
    }

    use frunk::{hlist, hlist_pat, HCons, HList, HNil};

    use crate::{
        graph::NodeHListOps,
        hierarchical_state::{False, True},
    };

    use super::{Broadcast, Node, Runnable, Subscribable, Subscription};
    use std::sync::mpsc::{Receiver, Sender};

    fn create_sender_and_node() -> (
        Sender<i64>,
        Node<i64, Receiver<i64>, (), i64, HNil, impl for<'a> Fn(i64, &'a ()) -> i64>,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let n = Node::new(|i: i64, _: &()| i + 1, receiver, ());
        (sender, n)
    }

    #[test]
    fn test_broadcast() {
        let (sender, n) = create_sender_and_node();
        let (n, r1) = n.add_subscription_channel();
        let (n, r2) = n.add_subscription_channel();
        let (n, r3) = n.add_subscription_channel();
        let b = (n.computation)(0, &());
        n.outputs.broadcast(b);
        assert_eq!(
            [r1.recv().unwrap(), r2.recv().unwrap(), r3.recv().unwrap()],
            [1, 1, 1]
        )
    }

    fn test_subscription() {
        let (sender, n) = create_sender_and_node();
        let node_list = hlist![n];
        let Subscription {
            subscribed,
            subscription: HCons { head, tail },
        } = Subscribable::<HList!(True)>::subscribe(node_list);
        let n1 = Node::new(|i: i64, _: &()| i + 2, head, ());
        let Subscription {
            subscribed,
            subscription: HCons { head, tail },
        } = Subscribable::<HList!(True)>::subscribe(subscribed);
        let n2 = Node::new(|i: i64, _| i + 3, head, ());
        let Subscription {
            subscribed,
            subscription: HCons { head, tail },
        } = Subscribable::<HList!(True)>::subscribe(subscribed);
        let n3 = Node::new(|i: i64, _| i + 4, head, ());

        let node_list_2 = hlist!(n1, n2, n3);
        let node_list_2 = node_list_2.prepend(subscribed.head);
        let Subscription {
            subscribed,
            subscription,
        } = Subscribable::<HList!(False, True, True, True)>::subscribe(node_list_2);
        // Then we check that it compiles to the right type
        let hlist_pat!(join1, join2, join3, join4) = subscribed.run();
        sender.send(0).unwrap();
        let hlist_pat!(x1, x2, x3) = subscription.receive().unwrap();
        assert_eq!((x1, x2, x3), (3, 4, 5));
        join1.join().unwrap().unwrap();
        join2.join().unwrap().unwrap();
        join3.join().unwrap().unwrap();
        join4.join().unwrap().unwrap();
    }

    #[test]
    fn test_node_subscription() {
        let (sender, n) = create_sender_and_node();
        let node_list = hlist![n]
            .add_node::<HList!(True), _, _, _>(|hlist_pat!(i), _| i + 2, ())
            .add_node::<HList!(False, True), _, _, _>(|hlist_pat!(i), _| i + 3, ())
            .add_node::<HList!(False, False, True), _, _, _>(|hlist_pat!(i), _| i + 4, ())
            .add_node::<HList!(True, True, True, False), _, _, _>(
                |hlist_pat!(x1, x2, x3), _| (x1, x2, x3),
                (),
            );
        let Subscription {
            subscribed: graph,
            subscription: receiver,
        } = Subscribable::<HList!(True, False, False, False, False)>::subscribe(node_list);
        let hlist_pat!(join1, join2, join3, join4, join5) = graph.run();
        sender.send(0).unwrap();
        drop(sender);
        let b = receiver.receive().unwrap();
        assert_eq!(b, hlist!((5, 4, 3)));
        join1.join().unwrap().unwrap();
        join2.join().unwrap().unwrap();
        join3.join().unwrap().unwrap();
        join4.join().unwrap().unwrap();
        join5.join().unwrap().unwrap();
    }
}
