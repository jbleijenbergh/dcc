#[derive(Debug)]
pub enum SurfaceError {
    Lost,
    Outdated,
    Timeout,
    Other(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Brush,
    Eraser,
}

#[derive(Clone, Debug)]
pub struct LoadError {
    pub path: std::path::PathBuf,
    pub message: String,
}
