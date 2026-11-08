use editor::SaveRequest;

use crate::file_picker::FilePickerRequest;

pub enum Effect {
    Save(SaveRequest),
    FilePicker(FilePickerRequest),
}
