import 'dart:io';

import 'package:flutter/painting.dart';

/// iOS 26 system colors with one green accent, in a dark and a light set.
/// The light set follows the app redesign (Nov 2026): zinc greys, a deeper
/// green and hairline-bordered cards.
/// Content sits on plain surfaces; Liquid Glass ([glass], [glassEdge]) is
/// only for floating chrome: the tab bar, toolbar buttons, accessories.
///
/// The values follow the system appearance: [AppearanceScope] sets [dark]
/// and rebuilds the tree when it changes, so read them in `build`, never in
/// a `const`.
abstract final class Tokens {
  static bool dark = true;

  static Color _p(int d, int l) => Color(dark ? d : l);

  // Backgrounds: grouped (bg) and plain lists (plain); cells on top.
  static Color get bg => _p(0xFF000000, 0xFFF2F2F4);
  static Color get plain => _p(0xFF000000, 0xFFFFFFFF);
  static Color get cell => _p(0xFF1C1C1E, 0xFFFFFFFF);
  static Color get cell2 => _p(0xFF2C2C2E, 0xFFE9E9EE);
  static Color get fill => _p(0x3D767680, 0x1F767680);
  static Color get segSel => _p(0x8C787880, 0xFFFFFFFF);

  // Labels.
  static Color get label => _p(0xFFFFFFFF, 0xFF18181B);
  static Color get label2 => _p(0x99EBEBF5, 0xFF5F5F68);
  static Color get label3 => _p(0x4DEBEBF5, 0x4D3C3C43);
  static Color get sep => _p(0xA6545458, 0xFFE4E4E7);
  static Color get sepOpaque => _p(0xFF38383A, 0xFFC6C6C8);

  // Accent and system tints.
  static Color get accent => _p(0xFF6AD18A, 0xFF15764C);
  static Color get accentInk => _p(0xFF062211, 0xFFFFFFFF);
  static Color get accentTint => _p(0x2E6AD18A, 0xFFE3F1E9);

  /// Text on an accent tint (a chosen project in a menu).
  static Color get accentDeep => _p(0xFF8FE0A8, 0xFF0F5A39);
  static Color get red => _p(0xFFFF453A, 0xFFB42318);
  static Color get orange => _p(0xFFFF9F0A, 0xFFFF9500);
  static Color get gray => _p(0xFF636366, 0xFF8E8E93);
  /// Marks what an employee wrote (the bar on its ideas); desktop's MANAGER_MARK.
  static Color get managerMark => _p(0xFFB0956A, 0xFF84683A);

  /// Glyphs and quiet controls (tray chips, inactive tabs).
  static Color get ink2 => _p(0xFFD4D4D8, 0xFF3F3F46);

  // Liquid Glass for floating chrome.
  static Color get glass => _p(0x8C242426, 0x99FFFFFF);
  static Color get glassEdge => _p(0x24FFFFFF, 0xE6FFFFFF);
  static Color get glassSel => _p(0x1AFFFFFF, 0x0F000000);
  static List<BoxShadow> get glassShadow => dark
      ? const [BoxShadow(color: Color(0x80000000), blurRadius: 28, offset: Offset(0, 8))]
      : const [BoxShadow(color: Color(0x24000000), blurRadius: 28, offset: Offset(0, 8)), BoxShadow(color: Color(0x14000000), blurRadius: 3, offset: Offset(0, 1))];

  // The names the screens grew up with, mapped onto the system set.
  static Color get sidebar => cell;
  static Color get panel => cell;
  static Color get surface => cell;
  static Color get surface2 => cell2;
  static Color get border => sep;
  static Color get borderStrong => sepOpaque;
  static Color get text => label;
  static Color get text2 => label2;
  static Color get muted => _p(0x80EBEBF5, 0x803C3C43);
  static Color get accentText => accent;
  static Color get accentHalo => accentTint;
  static Color get attentionBg => cell;
  static Color get attentionBorder => accentTint;
  static Color get negative => red;
  static Color get warning => orange;
  static Color get devBadge => orange;
  static Color get focusRing => accent;

  static const radius = 12.0;
  static const cardRadius = 16.0;

  /// Code only (diffs, commands, file paths). Times use [tabular].
  static String get mono => Platform.isIOS ? 'Menlo' : 'monospace';

  static const tabular = [FontFeature.tabularFigures()];

  /// SF Pro titles: bold from 20 pt up, semibold below. [style] is kept for
  /// old callers; the italic serif is gone.
  static TextStyle title({double size = 28, FontStyle style = FontStyle.normal}) => TextStyle(
        fontSize: size,
        height: 1.2,
        color: label,
        fontWeight: size >= 20 ? FontWeight.w700 : FontWeight.w600,
        letterSpacing: size >= 28 ? 0.2 : null,
      );

  /// Grouped-list section header ("THIS PC"): small, semibold, spaced caps.
  static TextStyle eyebrow() => TextStyle(fontSize: 11.5, letterSpacing: 0.6, fontWeight: FontWeight.w600, color: label2, fontFeatures: tabular);

  /// Times and counts: SF with tabular figures.
  static TextStyle monoSmall({Color? color}) => TextStyle(fontSize: 13, color: color ?? label2, fontFeatures: tabular);
}
