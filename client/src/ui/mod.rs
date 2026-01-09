use bevy::prelude::*;

use button::ButtonClicked;
use menus::Menu;

use crate::ui::hint::HintTextContent;

pub mod button;
pub mod elements;
pub mod hint;
pub mod menus;
pub mod util;

pub struct OpenvoxelUiPlugin;

impl Plugin for OpenvoxelUiPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .init_state::<Menu>()
            .init_resource::<HintTextContent>()
            .init_resource::<UiVars>()
            .init_resource::<util::UiLabels>()
            .add_message::<ButtonClicked>()
            .add_systems(Startup, (
                UiVars::load,
            ))
            .add_systems(Update, (
                button::handle_menu_button_ix,
                hint::insert_hint_text_visuals,
                hint::update_hint_text_entities,
            ))
            .add_systems(OnEnter(Menu::Title), (
                menus::title::draw,
            ))
            .add_systems(OnEnter(Menu::Starting), (
                menus::starting::draw,
            ))
            .add_systems(Last, (
                menus::starting::update_progress_bar,
            ))
            .add_systems(First, (
                util::on_ui_label_add,
                util::on_ui_label_remove,
            ))
        ;
    }
}

#[derive(Resource)]
pub struct UiVars {
    pub font: Handle<Font>,
    pub menu_body_max_width: Val,
    pub menu_body_width: Val,
}

impl UiVars {
    pub fn font(&self) -> Handle<Font> {
        self.font.clone()
    }

    fn load(server: Res<AssetServer>, mut vars: ResMut<UiVars>) {
        vars.font = server.load("fonts/ReturnOfTheBossRegular-E407g.ttf");
    }
}

impl Default for UiVars {
    fn default() -> Self {
        Self {
            font: Handle::default(),
            menu_body_max_width: Val::Px(1280.0),
            menu_body_width: Val::Percent(80.0),
        }
    }
}

#[derive(Component, Clone, Default, Deref, Eq, PartialEq, Hash)]
pub struct UiLabel(pub String);

impl UiLabel {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}
