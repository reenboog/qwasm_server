pub trait Purge {
	fn new() -> Self;
	fn purge(&mut self)
	where
		Self: Sized,
	{
		*self = Self::new();
	}
}
