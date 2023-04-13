use std::{
    cell::UnsafeCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    task::Context,
};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

mod task {
    use std::{cell::UnsafeCell, rc::Rc, sync::mpsc::SyncSender};

    use super::{waker::RcWake, BoxFuture};

    pub struct Task {
        pub(crate) future: UnsafeCell<BoxFuture<'static, ()>>,
        pub(crate) task_sender: SyncSender<Rc<Task>>,
    }

    impl RcWake for Task {
        fn wake_by_ref(rc_self: &Rc<Self>) {
            // Implement `wake` by sending this task back onto the task channel
            // so that it will be polled again by the executor.
            let cloned = rc_self.clone();
            rc_self
                .task_sender
                .send(cloned)
                .expect("too many tasks queued");
        }
    }
}

pub mod waker {
    use std::{
        mem,
        rc::Rc,
        task::{RawWaker, RawWakerVTable, Waker},
    };

    pub trait RcWake {
        fn wake_by_ref(rc_self: &Rc<Self>);

        fn wake(self: Rc<Self>) {
            Self::wake_by_ref(&self)
        }
    }

    pub(super) fn waker_vtable<W: RcWake>() -> &'static RawWakerVTable {
        &RawWakerVTable::new(
            clone_arc_raw::<W>,
            wake_arc_raw::<W>,
            wake_by_ref_arc_raw::<W>,
            drop_arc_raw::<W>,
        )
    }

    pub fn waker<W>(wake: Rc<W>) -> Waker
    where
        W: RcWake + 'static,
    {
        let ptr = Rc::into_raw(wake).cast::<()>();

        unsafe { Waker::from_raw(RawWaker::new(ptr, waker_vtable::<W>())) }
    }

    #[allow(clippy::redundant_clone)] // The clone here isn't actually redundant.
    unsafe fn increase_refcount<T: RcWake>(data: *const ()) {
        // Retain Arc, but don't touch refcount by wrapping in ManuallyDrop
        let arc = mem::ManuallyDrop::new(Rc::<T>::from_raw(data.cast::<T>()));
        // Now increase refcount, but don't drop new refcount either
        let _arc_clone: mem::ManuallyDrop<_> = arc.clone();
    }

    // used by `waker_ref`
    unsafe fn clone_arc_raw<T: RcWake>(data: *const ()) -> RawWaker {
        increase_refcount::<T>(data);
        dbg!("clone");
        RawWaker::new(data, waker_vtable::<T>())
    }

    unsafe fn wake_arc_raw<T: RcWake>(data: *const ()) {
        let arc: Rc<T> = Rc::from_raw(data.cast::<T>());
        RcWake::wake(arc);
    }

    // used by `waker_ref`
    unsafe fn wake_by_ref_arc_raw<T: RcWake>(data: *const ()) {
        // Retain Arc, but don't touch refcount by wrapping in ManuallyDrop
        let arc = mem::ManuallyDrop::new(Rc::<T>::from_raw(data.cast::<T>()));
        RcWake::wake_by_ref(&arc);
    }

    unsafe fn drop_arc_raw<T: RcWake>(data: *const ()) {
        drop(Rc::<T>::from_raw(data.cast::<T>()))
    }
}

// mod waker_ref {
//     use std::{
//         marker::PhantomData,
//         mem::ManuallyDrop,
//         ops::Deref,
//         rc::Rc,
//         task::{RawWaker, Waker},
//     };

//     use super::waker::{waker_vtable, RcWake};

//     #[derive(Debug)]
//     pub struct WakerRef<'a> {
//         waker: ManuallyDrop<Waker>,
//         _marker: PhantomData<&'a ()>,
//     }

//     impl<'a> WakerRef<'a> {
//         /// Create a new [`WakerRef`] from a [`Waker`] reference.
//         #[inline]
//         pub fn new(waker: &'a Waker) -> Self {
//             // copy the underlying (raw) waker without calling a clone,
//             // as we won't call Waker::drop either.
//             let waker = ManuallyDrop::new(unsafe { core::ptr::read(waker) });
//             Self {
//                 waker,
//                 _marker: PhantomData,
//             }
//         }

//         #[inline]
//         pub fn new_unowned(waker: ManuallyDrop<Waker>) -> Self {
//             Self {
//                 waker,
//                 _marker: PhantomData,
//             }
//         }
//     }

//     impl Deref for WakerRef<'_> {
//         type Target = Waker;

//         #[inline]
//         fn deref(&self) -> &Waker {
//             &self.waker
//         }
//     }

//     #[inline]
//     pub fn waker_ref<W>(wake: &Rc<W>) -> WakerRef<'_>
//     where
//         W: RcWake,
//     {
//         let ptr = Rc::as_ptr(wake).cast::<()>();

//         let waker =
//             ManuallyDrop::new(unsafe { Waker::from_raw(RawWaker::new(ptr, waker_vtable::<W>())) });
//         WakerRef::new_unowned(waker)
//     }
// }

use self::task::Task;

pub struct Executer {
    task_sender: SyncSender<Rc<Task>>,
    ready_queue: Receiver<Rc<Task>>,
}

impl Executer {
    pub fn new() -> Self {
        // Maximum number of tasks to allow queueing in the channel at once.
        // This is just to make `sync_channel` happy, and wouldn't be present in
        // a real executor.
        const MAX_QUEUED_TASKS: usize = 10_000;
        let (task_sender, ready_queue) = sync_channel(MAX_QUEUED_TASKS);
        Self {
            task_sender,
            ready_queue,
        }
    }

    pub fn spawn(&self, future: impl Future<Output = ()> + 'static + Send) {
        let future = UnsafeCell::new(Box::pin(future));

        let task = Rc::new(Task {
            future,
            task_sender: self.task_sender.clone(),
        });

        self.task_sender.send(task).unwrap();
    }

    pub fn run(&self) {
        for task in self.ready_queue.try_iter() {
            let future = unsafe { &mut *task.future.get() };

            let waker = waker::waker(task);

            let context = &mut Context::from_waker(&waker);

            let _ = future.as_mut().poll(context);
        }
    }
}
