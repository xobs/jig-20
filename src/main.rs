mod cfti;
use cfti::testset;
use cfti::testplan;
use cfti::gui;

#[macro_use]
extern crate conrod;
use conrod::backend::piston::gfx::{GfxContext, G2dTexture, Texture, TextureSettings, Flip};
use conrod::backend::piston::{self, Window, WindowEvents, OpenGL};
use conrod::backend::piston::event::UpdateEvent;

extern crate find_folder;

fn main() {

    let test_set = cfti::testset::TestSet::new("ltc-tests").unwrap();
    println!("Test set: {:?}", test_set);
    let plan = test_set.get_dev(&"Program App".to_string()).unwrap();
    println!("Tests: {:?}", plan);

     const WIDTH: u32 = cfti::gui::WIN_W;
    const HEIGHT: u32 = cfti::gui::WIN_H;

    // Construct the window.
    let mut window: Window =
        piston::window::WindowSettings::new("Common Factory Test Infrastructure - UI Panel", [WIDTH, HEIGHT])
            .opengl(OpenGL::V3_2) // If not working, try `OpenGL::V2_1`.
            .samples(4)
            .exit_on_esc(true)
            .vsync(true)
            .build()
            .unwrap();

    // Create the event loop.
    let mut events = WindowEvents::new();

    // A demonstration of some state that we'd like to control with the App.
    let mut app = cfti::gui::DemoApp::new();

    // construct our `Ui`.
    let mut ui = conrod::UiBuilder::new([WIDTH as f64, HEIGHT as f64])
        .theme(cfti::gui::theme())
        .build();

    // Add a `Font` to the `Ui`'s `font::Map` from file.
    let assets = find_folder::Search::KidsThenParents(3, 5).for_folder("assets").unwrap();
    let font_path = assets.join("fonts/NotoSans/NotoSans-Regular.ttf");
    ui.fonts.insert_from_file(font_path).unwrap();

    // Create a texture to use for efficiently caching text on the GPU.
    let mut text_texture_cache = piston::window::GlyphCache::new(&mut window, WIDTH, HEIGHT);

    // Instantiate the generated list of widget identifiers.
    let ids = cfti::gui::Ids::new(ui.widget_id_generator());

    // Create our `conrod::image::Map` which describes each of our widget->image mappings.
    // In our case we only have one image, however the macro may be used to list multiple.
    //let image_map = cfti::gui::image_map(&ids, load_rust_logo(&mut window.context));
    // The image map describing each of our widget->image mappings (in our case, none).
    let image_map = conrod::image::Map::new();

    // Poll events from the window.
    while let Some(event) = window.next_event(&mut events) {

        // Convert the piston event to a conrod event.
        if let Some(e) = piston::window::convert_event(event.clone(), &window) {
            ui.handle_event(e);
        }

        event.update(|_| {
            let mut ui = ui.set_widgets();
            cfti::gui::gui(&mut ui, &ids, &mut app);
        });

        window.draw_2d(&event, |c, g| {
            if let Some(primitives) = ui.draw_if_changed() {
                fn texture_from_image<T>(img: &T) -> &T { img };
                piston::window::draw(c, g, primitives,
                                     &mut text_texture_cache,
                                     &image_map,
                                     texture_from_image);
            }
        });
    }
}