use async_trait::async_trait;
use std::{collections::HashMap, fmt::Debug, hash::Hash};
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PhaseError {
    #[error("Invalid phase configuration")]
    Configuration,
    #[error("Transition is not allowed")]
    InvalidTransition,
    #[error("Another transition is running")]
    Busy,
    #[error("Controller is cancelled, completed, or faulted")]
    Inactive,
    #[error("Phase operation failed: {0}")]
    Action(String),
}
#[derive(Clone, Debug)]
pub enum PhaseEvent<P> {
    Initialized(P),
    Transitioned { from: P, to: P },
    Cancelled,
    Reset,
    Completed(P),
    Faulted,
}
#[async_trait]
pub trait PhaseActions<P: Send + 'static>: Send {
    async fn guard(&mut self, _from: P, _to: P) -> Result<(), PhaseError> {
        Ok(())
    }
    async fn enter(&mut self, _phase: P) -> Result<(), PhaseError> {
        Ok(())
    }
    async fn exit(&mut self, _phase: P) -> Result<(), PhaseError> {
        Ok(())
    }
    async fn cleanup(&mut self) -> Result<(), PhaseError> {
        Ok(())
    }
}
struct State<P, A> {
    current: Option<P>,
    history: Vec<P>,
    inactive: bool,
    actions: A,
}
pub struct BasePhasedController<P, A> {
    initial: P,
    phases: HashMap<P, Vec<P>>,
    state: Mutex<State<P, A>>,
    events: broadcast::Sender<PhaseEvent<P>>,
}
impl<P, A> BasePhasedController<P, A>
where
    P: Copy + Eq + Hash + Debug + Send + Sync + 'static,
    A: PhaseActions<P>,
{
    pub fn new(
        initial: P,
        phases: impl IntoIterator<Item = (P, Vec<P>)>,
        actions: A,
    ) -> Result<Self, PhaseError> {
        let phases: HashMap<P, Vec<P>> = phases.into_iter().collect();
        if !phases.contains_key(&initial)
            || phases.values().flatten().any(|p| !phases.contains_key(p))
        {
            return Err(PhaseError::Configuration);
        }
        let (events, _) = broadcast::channel(32);
        Ok(Self {
            initial,
            phases,
            state: Mutex::new(State {
                current: None,
                history: vec![],
                inactive: false,
                actions,
            }),
            events,
        })
    }
    pub fn subscribe(&self) -> broadcast::Receiver<PhaseEvent<P>> {
        self.events.subscribe()
    }
    pub async fn current_phase(&self) -> Option<P> {
        self.state.lock().await.current
    }
    pub async fn initialize(&self) -> Result<(), PhaseError> {
        let mut state = self.state.try_lock().map_err(|_| PhaseError::Busy)?;
        if state.current.is_some() || state.inactive {
            return Err(PhaseError::Inactive);
        }
        if let Err(error) = state.actions.enter(self.initial).await {
            let _ = state.actions.cleanup().await;
            return Err(error);
        }
        state.current = Some(self.initial);
        let _ = self.events.send(PhaseEvent::Initialized(self.initial));
        Ok(())
    }
    pub async fn can_transition(&self, to: P) -> bool {
        let Ok(state) = self.state.try_lock() else {
            return false;
        };
        !state.inactive
            && state
                .current
                .is_some_and(|from| self.phases[&from].contains(&to))
    }
    pub async fn transition_to(&self, to: P) -> Result<(), PhaseError> {
        self.transition(Some(to), false).await
    }
    pub async fn next_phase(&self) -> Result<(), PhaseError> {
        self.transition(None, false).await
    }
    pub async fn previous_phase(&self) -> Result<(), PhaseError> {
        self.transition(None, true).await
    }
    async fn transition(&self, target: Option<P>, previous: bool) -> Result<(), PhaseError> {
        let mut state = self.state.try_lock().map_err(|_| PhaseError::Busy)?;
        if state.inactive {
            return Err(PhaseError::Inactive);
        }
        let from = state.current.ok_or(PhaseError::Inactive)?;
        let to = if previous {
            state.history.last().copied()
        } else {
            target.or_else(|| self.phases[&from].first().copied())
        }
        .ok_or(PhaseError::InvalidTransition)?;
        if !self.phases[&from].contains(&to) {
            return Err(PhaseError::InvalidTransition);
        }
        state.actions.guard(from, to).await?;
        if let Err(error) = state.actions.exit(from).await {
            state.inactive = true;
            let _ = state.actions.cleanup().await;
            let _ = self.events.send(PhaseEvent::Faulted);
            return Err(error);
        }
        if let Err(error) = state.actions.enter(to).await {
            let cleaned = state.actions.cleanup().await.is_ok();
            if !cleaned || state.actions.enter(from).await.is_err() {
                state.inactive = true;
                let _ = self.events.send(PhaseEvent::Faulted);
            }
            return Err(error);
        }
        state.current = Some(to);
        if previous {
            state.history.pop();
        } else {
            state.history.push(from);
        }
        let _ = self.events.send(PhaseEvent::Transitioned { from, to });
        if self.phases[&to].is_empty() {
            state.inactive = true;
            state.actions.cleanup().await?;
            let _ = self.events.send(PhaseEvent::Completed(to));
        }
        Ok(())
    }
    pub async fn cancel(&self) -> Result<(), PhaseError> {
        let mut state = self.state.try_lock().map_err(|_| PhaseError::Busy)?;
        if state.inactive {
            return Ok(());
        }
        state.inactive = true;
        state.actions.cleanup().await?;
        let _ = self.events.send(PhaseEvent::Cancelled);
        Ok(())
    }
    pub async fn reset(&self) -> Result<(), PhaseError> {
        let mut state = self.state.try_lock().map_err(|_| PhaseError::Busy)?;
        state.inactive = true;
        state.actions.cleanup().await?;
        state.actions.enter(self.initial).await?;
        state.current = Some(self.initial);
        state.history.clear();
        state.inactive = false;
        let _ = self.events.send(PhaseEvent::Reset);
        Ok(())
    }
}

pub fn permits<P: PartialEq>(edges: &[(P, P)], from: &P, to: &P) -> bool {
    edges.iter().any(|(a, b)| a == from && b == to)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Actions;
    #[async_trait]
    impl PhaseActions<u8> for Actions {}
    #[tokio::test]
    async fn lifecycle() {
        let controller =
            BasePhasedController::new(0, [(0, vec![1]), (1, vec![0, 2]), (2, vec![])], Actions)
                .unwrap();
        controller.initialize().await.unwrap();
        assert_eq!(
            controller.transition_to(2).await,
            Err(PhaseError::InvalidTransition)
        );
        controller.next_phase().await.unwrap();
        controller.previous_phase().await.unwrap();
        assert_eq!(controller.current_phase().await, Some(0));
        controller.cancel().await.unwrap();
        assert!(!controller.can_transition(1).await);
        controller.reset().await.unwrap();
        controller.next_phase().await.unwrap();
        controller.transition_to(2).await.unwrap();
        assert!(!controller.can_transition(0).await);
    }
}
