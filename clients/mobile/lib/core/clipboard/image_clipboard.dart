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

/// The name of a pasted image: "Pasted image.png", then "Pasted image 2.png",
/// … for the [taken] names already in the list, so two pastes can be told apart.
String pastedImageName(Iterable<String> taken, String extension) {
  final names = taken.toSet();
  var name = 'Pasted image.$extension';
  for (var n = 2; names.contains(name); n++) {
    name = 'Pasted image $n.$extension';
  }
  return name;
}

/// Writes pasted bytes to a temporary file, hands it to [upload], and deletes
/// it again whatever [upload] does.
Future<T> withPastedFile<T>(Uint8List data, String name, Future<T> Function(File file) upload) async {
  final dir = await Directory.systemTemp.createTemp('aurelius-paste');
  try {
    final file = File('${dir.path}/$name');
    await file.writeAsBytes(data);
    return await upload(file);
  } finally {
    await dir.delete(recursive: true).catchError((_) => dir);
  }
}

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
