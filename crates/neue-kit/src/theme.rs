//! The theme as a Masonry property, so widgets read it at paint time and a theme change
//! repaints them ([`masonry_core::app::RenderRoot::set_default_properties`]).

use masonry_core::core::{DefaultProperties, Property};
use neue_theme::{Appearance, Backdrop, Theme, Variant};

use crate::widgets::{
    BevelWidget, ButtonWidget, CalendarWidget, CellWidget, CollectionItemWidget, ColorWellWidget,
    ColumnHeaderWidget, DateFieldWidget, DesktopWidget, FieldWidget, HeaderWidget, HsAreaWidget,
    IconWidget, LabelWidget, MenuBarWidget, MenuRowWidget, MenuTitleWidget, MenuWidget,
    NoteButtonWidget, NotificationHostWidget, NotificationWidget, OsdWidget, PathWidget,
    PopoverWidget, ProgressWidget, RootWidget, RowsWidget, ScrollWidget, ScrollerWidget,
    SegmentedWidget, SeparatorWidget, SheetWidget, SliderWidget, SpecimenWidget, SpinnerWidget,
    SplitWidget, StackWidget, StepperWidget, TabViewWidget, TextViewWidget, ToggleWidget,
    ToolbarButtonWidget, ToolbarWidget,
};

/// The app's [`Theme`], as a default property of every neue-kit widget.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ThemeProp(pub(crate) Theme);

static DEFAULT_THEME: ThemeProp = ThemeProp(Theme {
    variant: Variant::new(Appearance::Light),
    reduced_motion: false,
});

impl Property for ThemeProp {
    fn static_default() -> &'static Self {
        &DEFAULT_THEME
    }
}

/// What the popup materials (menus, popovers) paint on, as a default property of their
/// widgets: [`Backdrop::Translucent`] in a window whose compositor blurs behind popups
/// ([`crate::Cx::has_blur`]), else the opaque tint. The palettes fall back to opaque
/// under increased contrast by themselves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct BackdropProp(pub(crate) Backdrop);

static DEFAULT_BACKDROP: BackdropProp = BackdropProp(Backdrop::Opaque);

impl Property for BackdropProp {
    fn static_default() -> &'static Self {
        &DEFAULT_BACKDROP
    }
}

/// Default properties carrying `theme` for every neue-kit widget type, with opaque popups.
pub(crate) fn default_properties(theme: Theme) -> DefaultProperties {
    default_properties_on(theme, Backdrop::Opaque)
}

/// Default properties carrying `theme` for every neue-kit widget type, and `backdrop` for
/// the popup materials.
pub(crate) fn default_properties_on(theme: Theme, backdrop: Backdrop) -> DefaultProperties {
    let mut props = DefaultProperties::new();
    props.insert::<MenuWidget, _>(BackdropProp(backdrop));
    props.insert::<PopoverWidget, _>(BackdropProp(backdrop));
    props.insert::<LabelWidget, _>(ThemeProp(theme));
    props.insert::<IconWidget, _>(ThemeProp(theme));
    props.insert::<MenuWidget, _>(ThemeProp(theme));
    props.insert::<ColorWellWidget, _>(ThemeProp(theme));
    props.insert::<DateFieldWidget, _>(ThemeProp(theme));
    props.insert::<CalendarWidget, _>(ThemeProp(theme));
    props.insert::<HsAreaWidget, _>(ThemeProp(theme));
    props.insert::<PathWidget, _>(ThemeProp(theme));
    props.insert::<PopoverWidget, _>(ThemeProp(theme));
    props.insert::<SheetWidget, _>(ThemeProp(theme));
    props.insert::<MenuRowWidget, _>(ThemeProp(theme));
    props.insert::<MenuBarWidget, _>(ThemeProp(theme));
    props.insert::<DesktopWidget, _>(ThemeProp(theme));
    props.insert::<OsdWidget, _>(ThemeProp(theme));
    props.insert::<NotificationWidget, _>(ThemeProp(theme));
    props.insert::<NotificationHostWidget, _>(ThemeProp(theme));
    props.insert::<NoteButtonWidget, _>(ThemeProp(theme));
    props.insert::<MenuTitleWidget, _>(ThemeProp(theme));
    props.insert::<ButtonWidget, _>(ThemeProp(theme));
    props.insert::<BevelWidget, _>(ThemeProp(theme));
    props.insert::<ToggleWidget, _>(ThemeProp(theme));
    props.insert::<SegmentedWidget, _>(ThemeProp(theme));
    props.insert::<SliderWidget, _>(ThemeProp(theme));
    props.insert::<StepperWidget, _>(ThemeProp(theme));
    props.insert::<ProgressWidget, _>(ThemeProp(theme));
    props.insert::<SpinnerWidget, _>(ThemeProp(theme));
    props.insert::<FieldWidget, _>(ThemeProp(theme));
    props.insert::<TextViewWidget, _>(ThemeProp(theme));
    props.insert::<SpecimenWidget, _>(ThemeProp(theme));
    props.insert::<SplitWidget, _>(ThemeProp(theme));
    props.insert::<StackWidget, _>(ThemeProp(theme));
    props.insert::<TabViewWidget, _>(ThemeProp(theme));
    props.insert::<ToolbarButtonWidget, _>(ThemeProp(theme));
    props.insert::<ToolbarWidget, _>(ThemeProp(theme));
    props.insert::<SeparatorWidget, _>(ThemeProp(theme));
    props.insert::<RootWidget, _>(ThemeProp(theme));
    props.insert::<ScrollWidget, _>(ThemeProp(theme));
    props.insert::<ScrollerWidget, _>(ThemeProp(theme));
    props.insert::<RowsWidget, _>(ThemeProp(theme));
    props.insert::<HeaderWidget, _>(ThemeProp(theme));
    props.insert::<ColumnHeaderWidget, _>(ThemeProp(theme));
    props.insert::<CellWidget, _>(ThemeProp(theme));
    props.insert::<CollectionItemWidget, _>(ThemeProp(theme));
    props
}
