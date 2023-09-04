#[derive(Debug, PartialEq)]
pub struct GitIgnoreIn {
    pub(crate) content: Vec<GitIgnoreStatement>,
}

#[derive(Debug, PartialEq)]
pub enum GitIgnoreStatement {
    Comment(Comment),
    Meaningless(Meaningless),
    Gibo(Gibo),
    Gi(Gi),
    Echo(Echo),
}

#[derive(Debug, PartialEq)]
pub enum Comment {
    Content(String),
}

#[derive(Debug, PartialEq)]
pub enum Meaningless {
    Content(String),
}

#[derive(Debug, PartialEq)]
pub enum Gibo {
    Target(String),
}

#[derive(Debug, PartialEq)]
pub enum Gi {
    Target(String),
}

#[derive(Debug, PartialEq)]
pub enum Echo {
    Content(String),
}
