use super::opencode::Integration as OpenCodeIntegration;
use super::{AgentConfigContext, AgentIntegration};

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "kilocode"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        OpenCodeIntegration.apply(ctx)
    }
}
