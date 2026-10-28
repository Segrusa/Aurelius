import 'package:flutter/cupertino.dart' show CupertinoPageTransitionsBuilder, NoDefaultCupertinoThemeData;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'tokens.dart';

/// The theme for the current [Tokens.dark]: iOS 26 grouped surfaces, SF
/// text, pill-shaped controls and one green accent.
ThemeData aureliusTheme() {
  final dark = Tokens.dark;
  final scheme = (dark ? const ColorScheme.dark() : const ColorScheme.light()).copyWith(
    surface: Tokens.bg,
    onSurface: Tokens.label,
    onSurfaceVariant: Tokens.label2,
    primary: Tokens.accent,
    onPrimary: Tokens.accentInk,
    primaryContainer: Tokens.accentTint,
    onPrimaryContainer: Tokens.accent,
    secondary: Tokens.accent,
    onSecondary: Tokens.accentInk,
    secondaryContainer: Tokens.fill,
    onSecondaryContainer: Tokens.label,
    error: Tokens.red,
    onError: Colors.white,
    outline: Tokens.sep,
    outlineVariant: Tokens.sep,
    surfaceContainerLowest: Tokens.bg,
    surfaceContainerLow: Tokens.cell,
    surfaceContainer: Tokens.cell,
    surfaceContainerHigh: Tokens.cell,
    surfaceContainerHighest: Tokens.cell2,
  );
  final base = ThemeData(useMaterial3: true, colorScheme: scheme, brightness: dark ? Brightness.dark : Brightness.light);
  final pill = RoundedRectangleBorder(borderRadius: BorderRadius.circular(999));
  final overlay = dark ? SystemUiOverlayStyle.light : SystemUiOverlayStyle.dark;
  return base.copyWith(
    // Swipe back from the edge on iOS; Android's predictive back gesture.
    pageTransitionsTheme: const PageTransitionsTheme(builders: {
      TargetPlatform.iOS: CupertinoPageTransitionsBuilder(),
      TargetPlatform.android: PredictiveBackPageTransitionsBuilder(),
    }),
    scaffoldBackgroundColor: Tokens.bg,
    canvasColor: Tokens.bg,
    dividerColor: Tokens.sep,
    dividerTheme: DividerThemeData(color: Tokens.sep, thickness: 0.5, space: 0.5),
    splashFactory: NoSplash.splashFactory,
    highlightColor: Tokens.fill,
    cupertinoOverrideTheme: NoDefaultCupertinoThemeData(primaryColor: Tokens.accent, brightness: base.brightness),
    textTheme: base.textTheme.apply(bodyColor: Tokens.label, displayColor: Tokens.label).copyWith(
          bodyLarge: TextStyle(fontSize: 17, height: 1.3, color: Tokens.label),
          bodyMedium: TextStyle(fontSize: 16, height: 1.45, color: Tokens.label),
          bodySmall: TextStyle(fontSize: 13, height: 1.35, color: Tokens.label2),
          titleMedium: TextStyle(fontSize: 17, height: 1.3, color: Tokens.label),
          labelSmall: TextStyle(fontSize: 11, color: Tokens.label2),
        ),
    iconTheme: IconThemeData(color: Tokens.label),
    appBarTheme: AppBarTheme(
      backgroundColor: Tokens.bg,
      foregroundColor: Tokens.label,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: true,
      systemOverlayStyle: overlay.copyWith(statusBarColor: Colors.transparent),
      titleTextStyle: TextStyle(fontSize: 17, fontWeight: FontWeight.w600, color: Tokens.label),
      iconTheme: IconThemeData(color: Tokens.label, size: 21),
      actionsIconTheme: IconThemeData(color: Tokens.label, size: 21),
    ),
    cardTheme: CardThemeData(
      color: Tokens.cell,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius)),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: Tokens.accent,
        foregroundColor: Tokens.accentInk,
        disabledBackgroundColor: Tokens.fill,
        disabledForegroundColor: Tokens.label3,
        shape: pill,
        minimumSize: const Size(0, 44),
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 17),
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 11),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: Tokens.label,
        backgroundColor: Tokens.fill,
        side: BorderSide.none,
        shape: pill,
        minimumSize: const Size(0, 44),
        textStyle: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600),
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 11),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(foregroundColor: Tokens.accent, shape: pill, textStyle: const TextStyle(fontSize: 17)),
    ),
    iconButtonTheme: IconButtonThemeData(style: IconButton.styleFrom(foregroundColor: Tokens.label)),
    floatingActionButtonTheme: FloatingActionButtonThemeData(
      backgroundColor: Tokens.accent,
      foregroundColor: Tokens.accentInk,
      elevation: 0,
      highlightElevation: 0,
      // Stadium: a circle for the round button, a pill for extended ones.
      shape: const StadiumBorder(),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: Tokens.cell,
      hintStyle: TextStyle(color: Tokens.label3),
      labelStyle: TextStyle(color: Tokens.label2),
      contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(Tokens.radius), borderSide: BorderSide.none),
      enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(Tokens.radius), borderSide: BorderSide.none),
      focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(Tokens.radius), borderSide: BorderSide(color: Tokens.accent, width: 1.5)),
    ),
    chipTheme: base.chipTheme.copyWith(
      backgroundColor: Tokens.fill,
      selectedColor: Tokens.accentTint,
      side: BorderSide.none,
      labelStyle: TextStyle(fontSize: 14, color: Tokens.label),
      shape: pill,
    ),
    bottomSheetTheme: BottomSheetThemeData(
      backgroundColor: Tokens.dark ? Tokens.cell : Tokens.bg,
      surfaceTintColor: Colors.transparent,
      showDragHandle: true,
      dragHandleColor: Tokens.label3,
      shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(34))),
    ),
    // Phone dialogs use nearly the full width (Material leaves 40 px a side).
    dialogTheme: DialogThemeData(
      backgroundColor: Tokens.dark ? Tokens.cell : Tokens.plain,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius)),
      titleTextStyle: TextStyle(fontSize: 17, fontWeight: FontWeight.w600, color: Tokens.label),
      contentTextStyle: TextStyle(fontSize: 15, height: 1.35, color: Tokens.label2),
      insetPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 24),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: Tokens.dark ? Tokens.cell2 : Tokens.plain,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      textStyle: TextStyle(fontSize: 17, color: Tokens.label),
    ),
    snackBarTheme: SnackBarThemeData(
      backgroundColor: Tokens.dark ? Tokens.cell2 : const Color(0xFF1C1C1E),
      contentTextStyle: const TextStyle(color: Colors.white, fontSize: 15),
      behavior: SnackBarBehavior.floating,
      shape: pill,
    ),
    listTileTheme: ListTileThemeData(
      iconColor: Tokens.label2,
      textColor: Tokens.label,
      titleTextStyle: TextStyle(fontSize: 17, color: Tokens.label),
      subtitleTextStyle: TextStyle(fontSize: 15, color: Tokens.label2),
    ),
    switchTheme: SwitchThemeData(
      thumbColor: const WidgetStatePropertyAll(Colors.white),
      trackColor: WidgetStateProperty.resolveWith((s) => s.contains(WidgetState.selected) ? Tokens.accent : Tokens.fill),
      trackOutlineColor: const WidgetStatePropertyAll(Colors.transparent),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: Tokens.label2, linearTrackColor: Tokens.fill),
    tabBarTheme: TabBarThemeData(
      labelColor: Tokens.label,
      unselectedLabelColor: Tokens.label2,
      indicatorColor: Tokens.accent,
      dividerColor: Colors.transparent,
    ),
    segmentedButtonTheme: SegmentedButtonThemeData(
      style: SegmentedButton.styleFrom(
        backgroundColor: Tokens.fill,
        selectedBackgroundColor: Tokens.segSel,
        foregroundColor: Tokens.label,
        selectedForegroundColor: Tokens.label,
        side: BorderSide.none,
        shape: pill,
      ),
    ),
  );
}
