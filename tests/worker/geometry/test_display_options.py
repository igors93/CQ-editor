"""Color and opacity inputs accepted by show_object."""

import pytest

from cq_worker.scene import DisplayOptions, parse_color


@pytest.mark.parametrize(
    ("color", "rgb", "alpha"),
    [
        ("#f00", (1, 0, 0), None),
        ("#11223344", (0x11 / 255, 0x22 / 255, 0x33 / 255), 0x44 / 255),
        ([0, 128, 255], (0, 128 / 255, 1), None),
        ((0.2, 0.4, 0.6, 0.8), (0.2, 0.4, 0.6), 0.8),
    ],
)
def test_supported_color_forms(color, rgb, alpha):
    actual_rgb, actual_alpha = parse_color(color)
    assert actual_rgb == pytest.approx(rgb)
    assert actual_alpha == (pytest.approx(alpha) if alpha is not None else None)


@pytest.mark.parametrize("color", ["#12", "#zzzzzz", (-1, 0, 0), (0.0, 1.1, 0.0)])
def test_invalid_color_forms_are_rejected(color):
    with pytest.raises(ValueError):
        parse_color(color)


@pytest.mark.parametrize(("input_alpha", "expected"), [(-2, 0), (0.25, 0.25), (5, 1)])
def test_display_opacity_is_clamped(input_alpha, expected):
    assert DisplayOptions.parse({"alpha": input_alpha}).alpha == expected


def test_explicit_opacity_overrides_color_opacity():
    options = DisplayOptions.parse({"color": "#ff000080", "alpha": 0.9})
    assert options.color == pytest.approx((1, 0, 0))
    assert options.alpha == 0.9


def test_display_options_message_has_all_fields():
    assert DisplayOptions.parse(None).to_message() == {
        "color": None,
        "alpha": None,
        "edge_color": None,
    }
