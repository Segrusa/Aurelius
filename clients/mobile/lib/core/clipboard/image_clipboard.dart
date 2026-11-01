import 'dart:io';

import 'package:flutter/material.dart';
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

/// The image types a keyboard (Gboard) may insert into a field that takes images.
const pastedImageMimeTypes = ['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/bmp'];

/// The file extension for an image a keyboard inserted, by its MIME type.
String pastedImageExtension(String mimeType) => switch (mimeType) {
      'image/png' => 'png',
      'image/gif' => 'gif',
      'image/webp' => 'webp',
      'image/bmp' => 'bmp',
      _ => 'jpg',
    };

/// A field's edit menu with Paste Image after Paste (Flutter's own Paste only
/// handles text), or Paste on top when there is no text to paste.
List<ContextMenuButtonItem> withPasteImage(List<ContextMenuButtonItem> items, VoidCallback onPasteImage) {
  final at = items.indexWhere((i) => i.type == ContextMenuButtonType.paste);
  final paste = ContextMenuButtonItem(
    label: at < 0 ? 'Paste' : 'Paste Image',
    onPressed: () {
      ContextMenuController.removeAny();
      onPasteImage();
    },
  );
  return [...items]..insert(at < 0 ? 0 : at + 1, paste);
}
