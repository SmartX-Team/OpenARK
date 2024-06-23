use serde::{Deserialize, Serialize};

use crate::problem::ProblemSpec;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradableProblem {
    pub spec: ProblemSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradableFunction {}

// TODO: TradableProblem의 UID 같은 인식가능한 토큰을 정의하자.
// TODO: 토큰이 별다른 짓 없이 공유되었으면 좋겠는데, 최선은 역시 마켓 problem을 인용하는 것일까?
