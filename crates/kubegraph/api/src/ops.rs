pub trait Eq<Rhs = Self> {
    type Output;

    fn eq(self, rhs: Rhs) -> Self::Output;
}

pub trait Ne<Rhs = Self> {
    type Output;

    fn ne(self, rhs: Rhs) -> Self::Output;
}

pub trait Ge<Rhs = Self> {
    type Output;

    fn ge(self, rhs: Rhs) -> Self::Output;
}

pub trait Gt<Rhs = Self> {
    type Output;

    fn gt(self, rhs: Rhs) -> Self::Output;
}

pub trait Le<Rhs = Self> {
    type Output;

    fn le(self, rhs: Rhs) -> Self::Output;
}

pub trait Lt<Rhs = Self> {
    type Output;

    fn lt(self, rhs: Rhs) -> Self::Output;
}

pub trait And<Rhs = Self> {
    type Output;

    fn and(self, rhs: Rhs) -> Self::Output;
}

pub trait Or<Rhs = Self> {
    type Output;

    fn or(self, rhs: Rhs) -> Self::Output;
}
