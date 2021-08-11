use std::marker::PhantomData;

pub struct Sender<T> {
    _phantom: PhantomData<T>,
}

pub struct Receiver<T> {
    _phantom: PhantomData<T>,
}
