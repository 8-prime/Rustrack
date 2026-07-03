use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen]
pub fn renderer(canvas: HtmlCanvasElement) {
    web_sys::console::log_1(&canvas);
}
