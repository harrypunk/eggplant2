use editor::SaveRequest;

pub enum Effect {
    Save(SaveRequest),
}
