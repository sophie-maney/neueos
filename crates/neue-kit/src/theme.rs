//! The theme as a Masonry property, so widgets read it at paint time and a theme change
//! repaints them ([`masonry_core::app::RenderRoot::set_default_properties`]).

use masonry_core::core::{DefaultProperties, Property};
use neue_theme::{Appearance, Theme, Variant};

use crate::widgets::{
    BevelWidget, ButtonWidget, CellWidget, CollectionItemWidget, ColumnHeaderWidget, FieldWidget,
    HeaderWidget, IconWidget, LabelWidget, MenuRowWidget, MenuWidget, ProgressWidget, RootWidget,
    RowsWidget, ScrollWidget, ScrollerWidget, SegmentedWidget, SliderWidget, SpecimenWidget,
    SpinnerWidget, SplitWidget, StackWidget, StepperWidget, TextViewWidget, ToggleWidget,
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

/// Default properties carrying `theme` for every neue-kit widget type.
pub(crate) fn default_properties(theme: Theme) -> DefaultProperties {
    let mut props = DefaultProperties::new();
    props.insert::<LabelWidget, _>(ThemeProp(theme));
    props.insert::<IconWidget, _>(ThemeProp(theme));
    props.insert::<MenuWidget, _>(ThemeProp(theme));
    props.insert::<MenuRowWidget, _>(ThemeProp(theme));
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
