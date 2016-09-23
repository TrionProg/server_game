
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum GameState{
    Initializing,
    Initialized,
    GeneratingMap,
    LoadingMap,
    InGame,
    Shutdown,
    Error,
}
