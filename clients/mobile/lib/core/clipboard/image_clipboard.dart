import 'dart:io';

import 'package:flutter/services.dart';

/// Images on the system clipboard. Flutter's text fields only paste text, so
/// a copied photo or screenshot is read natively (UIPasteboard on iOS,
/// ClipboardManager on Android).
class ImageClipboard {
  ImageClipboard({MethodChannel? channel}) : _channel = channel ?? const MethodChannel('aurelius/clipboard');

  final MethodChannel _channel;

  /// Whether the clipboard holds an image. Does not read it, so iOS shows
  /// no paste prompt for this.
  Future<bool> hasImage() async {
    if (!Platform.isIOS && !Platform.isAndroid) return false;
    try {
      return await _channel.invokeMethod<bool>('hasImage') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  /// The image as PNG or JPEG bytes with its extension, or null.
  Future<(Uint8List, String)?> read() async {
    try {
      final result = await _channel.invokeMapMethod<String, Object?>('image');
      final bytes = result?['bytes'];
      if (bytes is! Uint8List || bytes.isEmpty) return null;
      return (bytes, result?['extension'] as String? ?? 'png');
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }
}
