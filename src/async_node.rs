use std::{future::Future, pin::Pin};

use frunk::{prelude::HList, HCons, HNil};
use pin_project::pin_project;
use tokio::sync::mpsc::channel;

use tokio::join;

trait FutureHList {}

struct FutureHNil;
impl FutureHList for FutureHNil {}

#[derive(Clone)]
#[pin_project]
struct FutureHCons<HeadOut, TailOut, HeadFut, TailFut> {
    #[pin]
    head: HeadFut,
    #[pin]
    tail: TailFut,
    head_out: std::task::Poll<HeadOut>,
    tail_out: std::task::Poll<TailOut>,
}
impl<HeadOut, TailOut, HeadFut, TailFut: FutureHList> FutureHList
    for FutureHCons<HeadOut, TailOut, HeadFut, TailFut>
{
}

impl Future for FutureHNil {
    type Output = HNil;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(HNil)
    }
}

impl<O, Fut: Future<Output = O>, TailOut, TailFuture: FutureHList + Future<Output = TailOut>> Future
    for FutureHCons<O, TailOut, Fut, TailFuture>
{
    type Output = HCons<O, TailOut>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        let mut resp: &mut std::task::Poll<O> = this.head_out;
        let mut resp2: &mut std::task::Poll<TailOut> = this.tail_out;
        let mut output = std::task::Poll::Pending;
        match (&mut resp, &mut resp2) {
            (std::task::Poll::Pending, std::task::Poll::Ready(_)) => {
                *resp = this.head.poll(cx);
            }
            (std::task::Poll::Ready(_), std::task::Poll::Pending) => {
                *resp2 = this.tail.poll(cx);
            }
            (std::task::Poll::Pending, std::task::Poll::Pending) => {
                *resp = this.head.poll(cx);
                *resp2 = this.tail.poll(cx);
            }
            (std::task::Poll::Ready(_), std::task::Poll::Ready(_)) => {
                let head_poll: std::task::Poll<O> = std::task::Poll::Pending;
                let tail_poll: std::task::Poll<TailOut> = std::task::Poll::Pending;
                let head_poll = std::mem::replace(resp, head_poll);
                let tail_poll = std::mem::replace(resp2, tail_poll);
                if let (std::task::Poll::Ready(head), std::task::Poll::Ready(tail)) =
                    (head_poll, tail_poll)
                {
                    output = std::task::Poll::Ready(HCons { head, tail });
                }
            }
        }
        if output.is_pending() {
            cx.waker().clone().wake();
            output
        } else {
            output
        }
    }
}

trait HListToFuture {
    type HListFutureType: FutureHList;
    fn to_hlist_future(self) -> Self::HListFutureType;
}

impl HListToFuture for HNil {
    type HListFutureType = FutureHNil;

    fn to_hlist_future(self) -> FutureHNil {
        FutureHNil
    }
}

impl<
        HeadOut,
        TailOut,
        HeadFut: Future<Output = HeadOut>,
        TailFutureHList: FutureHList + Future<Output = TailOut>,
        HListOfFut: HListToFuture<HListFutureType = TailFutureHList>,
    > HListToFuture for HCons<HeadFut, HListOfFut>
{
    type HListFutureType = FutureHCons<HeadOut, TailOut, HeadFut, TailFutureHList>;

    fn to_hlist_future(self) -> Self::HListFutureType {
        let HCons { head, tail } = self;
        let future_tail = tail.to_hlist_future();
        FutureHCons {
            head,
            tail: future_tail,
            head_out: std::task::Poll::Pending,
            tail_out: std::task::Poll::Pending,
        }
    }
}
#[cfg(test)]
mod test {
    use super::HListToFuture;
    use frunk::hlist;
    use futures::StreamExt;
    use futures_timer::Delay;
    use std::time::Duration;
    use tokio_stream::wrappers::UnboundedReceiverStream;
    async fn push(i: u64, sender: tokio::sync::mpsc::UnboundedSender<u64>) {
        let sender = sender.clone();
        Delay::new(Duration::from_micros(i * 2000u64)).await;
        println!("{i}");
        sender.send(i).unwrap();
    }

    #[tokio::test]
    async fn test_order() {
        // let vec = std::sync::Arc::new(std::sync::Mutex::new(Vec::with_capacity(10)));
        // let vec = Rc::new(RefCell::new(Vec::with_capacity(10)));
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        let fut_1 = push(1, sender.clone());
        let fut_2 = push(2, sender.clone());
        let fut_3 = push(3, sender.clone());
        let fut_4 = push(4, sender.clone());
        hlist![fut_1, fut_2, fut_3, fut_4].to_hlist_future().await;
        let fut_1 = push(9, sender.clone());
        let fut_2 = push(8, sender.clone());
        let fut_3 = push(7, sender.clone());
        let fut_4 = push(6, sender.clone());
        let fut_5 = push(5, sender.clone());

        hlist!(fut_1, fut_2, fut_3, fut_4, fut_5)
            .to_hlist_future()
            .await;
        drop(sender);

        let vec = UnboundedReceiverStream::new(receiver)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(vec, (1..=9).collect::<Vec<_>>());
    }
}
