#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitIgnoreIn {
    pub(crate) content: Vec<GitIgnoreStatement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitIgnoreStatement {
    Comment(Comment),
    Meaningless(Meaningless),
    Gibo(Gibo),
    Gi(Gi),
    Echo(Echo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comment {
    Content(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Meaningless {
    Content(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gibo {
    Target(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gi {
    Target(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Echo {
    Content(String),
}
