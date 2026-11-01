import 'package:aurelius_mobile/core/clipboard/image_clipboard.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Paste Image comes right after the text Paste', () {
    final items = [
      ContextMenuButtonItem(type: ContextMenuButtonType.copy, onPressed: () {}),
      ContextMenuButtonItem(type: ContextMenuButtonType.paste, onPressed: () {}),
      ContextMenuButtonItem(type: ContextMenuButtonType.selectAll, onPressed: () {}),
    ];
    final menu = withPasteImage(items, () {});
    expect(menu.map((i) => i.label ?? i.type.name), ['copy', 'paste', 'Paste Image', 'selectAll']);
    expect(items, hasLength(3), reason: 'the field\'s own list is not changed');
  });

  test('without text to paste, the image paste is the Paste on top', () {
    final menu = withPasteImage([ContextMenuButtonItem(type: ContextMenuButtonType.selectAll, onPressed: () {})], () {});
    expect(menu.first.label, 'Paste');
  });

  test('an inserted image keeps the extension of its type, jpg otherwise', () {
    expect(pastedImageExtension('image/png'), 'png');
    expect(pastedImageExtension('image/webp'), 'webp');
    expect(pastedImageExtension('image/jpeg'), 'jpg');
    expect(pastedImageExtension('image/heic'), 'jpg');
  });
}
