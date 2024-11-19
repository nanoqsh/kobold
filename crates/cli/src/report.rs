use std::error;
use std::fmt;

#[derive(Debug)]
pub struct Report<
    E = Box<
        dyn error::Error //
            + Send //  anyhow requirement
            + Sync, // anyhow requirement
    >,
> {
    err: Option<E>,
    message: String,
}

impl<E> Report<E> {
    pub fn new<U, M>(err: U, message: M) -> Self
    where
        U: Into<E>,
        M: Into<String>,
    {
        Self {
            err: Some(err.into()),
            message: message.into(),
        }
    }

    pub fn message<M>(message: M) -> Self
    where
        M: Into<String>,
    {
        Self {
            err: None,
            message: message.into(),
        }
    }
}

impl<E> fmt::Display for Report<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(err) = &self.err {
            write!(f, ": {err}")?;
        }

        Ok(())
    }
}

pub trait ReportExt<T, E> {
    fn with_message<M>(self, message: M) -> Result<T, Report<E>>
    where
        M: Into<String>;
}

impl<T, E, U> ReportExt<T, E> for Result<T, U>
where
    U: Into<E>,
{
    fn with_message<M>(self, message: M) -> Result<T, Report<E>>
    where
        M: Into<String>,
    {
        self.map_err(|err| Report::new(err, message))
    }
}

pub struct DisplayError<E>(pub E);

impl<E> fmt::Display for DisplayError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<E> fmt::Debug for DisplayError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DisplayError").field(&"..").finish()
    }
}

impl<E> error::Error for DisplayError<E> where E: fmt::Display {}
