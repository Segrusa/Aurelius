import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/clipboard/image_clipboard.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../project/new_task_page.dart' show maxTaskFileBytes;

/// What the ticket's title or description dialog gives back: the text, and the
/// upload paths of the images pasted into it (ticket #3).
typedef TicketTextEdit = ({String text, List<String> files});

/// The dialog that edits a ticket's title or description. With [room] above
/// zero (the server takes files on an existing ticket, and the ticket has fewer
/// than the most), a pasted image (Paste Image, a keyboard's image, ⌘/Ctrl+⇧V)
/// is uploaded at once and listed under the field, to be added on Save.
/// Returns `null` when cancelled.
Future<TicketTextEdit?> editTicketText(BuildContext context, {required String label, required String initial, int lines = 1, int? maxLength, bool required = true, int room = 0}) {
  return showDialog<TicketTextEdit>(
    context: context,
    builder: (_) => TicketTextDialog(label: label, initial: initial, lines: lines, maxLength: maxLength, required: required, room: room),
  );
}

/// See [editTicketText].
class TicketTextDialog extends StatefulWidget {
  const TicketTextDialog({super.key, required this.label, required this.initial, this.lines = 1, this.maxLength, this.required = true, this.room = 0});

  final String label;
  final String initial;
  final int lines;
  final int? maxLength;
  final bool required;

  /// How many files the ticket still takes; 0 turns pasting images off.
  final int room;

  @override
  State<TicketTextDialog> createState() => _TicketTextDialogState();
}

class _TicketTextDialogState extends State<TicketTextDialog> {
  late final _text = TextEditingController(text: widget.initial);
  final _focus = FocusNode();
  final _clipboard = ImageClipboard();
  final _files = <({String name, String path})>[];

  /// Whether the clipboard holds an image, for Paste Image in the edit menu
  /// (the menu is built synchronously, so this is checked on focus and tap).
  bool _clipboardImage = false;
  bool _uploading = false;

  bool get _pastes => widget.room > 0;

  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
    _focus.addListener(() {
      if (_focus.hasFocus) _checkClipboard();
    });
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  Future<void> _checkClipboard() async {
    if (!_pastes) return;
    final has = await _clipboard.hasImage();
    if (mounted && has != _clipboardImage) setState(() => _clipboardImage = has);
  }

  Future<void> _pasteImage() async {
    if (!_pastes || _uploading) return;
    final image = await _clipboard.read();
    if (!mounted) return;
    if (image == null) return toast(context, 'There is no image on the clipboard.');
    await _addImage(image.$1, image.$2);
  }

  Future<void> _insert(KeyboardInsertedContent content) async {
    final data = content.data;
    if (data == null || data.isEmpty) return toast(context, 'This image could not be read.');
    await _addImage(data, pastedImageExtension(content.mimeType));
  }

  /// Keep the pasted bytes as a file and upload it to the PC at once.
  Future<void> _addImage(Uint8List data, String extension) async {
    if (_files.length >= widget.room) return toast(context, 'This ticket takes no more files.');
    if (data.length > maxTaskFileBytes) return toast(context, 'The image is larger than 25 MB and was not added.');
    final repository = context.read<AureliusRepository>();
    final name = pastedImageName(_files.map((f) => f.name), extension);
    setState(() => _uploading = true);
    try {
      final uploaded = await withPastedFile(data, name, (file) => repository.upload(file, name));
      if (mounted) setState(() => _files.add((name: uploaded.name, path: uploaded.path)));
    } catch (e) {
      if (mounted) toast(context, '$name: $e');
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  Widget _menu(BuildContext context, EditableTextState field) {
    final items = _clipboardImage ? withPasteImage(field.contextMenuButtonItems, _pasteImage) : field.contextMenuButtonItems;
    return AdaptiveTextSelectionToolbar.buttonItems(anchors: field.contextMenuAnchors, buttonItems: items);
  }

  @override
  Widget build(BuildContext context) {
    final text = _text.text.trim();
    final valid = !_uploading && (!widget.required || text.isNotEmpty);
    return CallbackShortcuts(
      bindings: {
        if (_pastes) const SingleActivator(LogicalKeyboardKey.keyV, meta: true, shift: true): _pasteImage,
        if (_pastes) const SingleActivator(LogicalKeyboardKey.keyV, control: true, shift: true): _pasteImage,
      },
      child: AlertDialog(
        title: Text(widget.label),
        content: SizedBox(
          width: double.maxFinite,
          child: SingleChildScrollView(
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
              TextField(
                key: const ValueKey('ticket-text-field'),
                controller: _text,
                focusNode: _focus,
                autofocus: true,
                minLines: widget.lines,
                maxLines: widget.lines == 1 ? 1 : widget.lines + 4,
                maxLength: widget.maxLength,
                textCapitalization: TextCapitalization.sentences,
                onTap: _checkClipboard,
                contextMenuBuilder: _pastes ? _menu : null,
                contentInsertionConfiguration: _pastes ? ContentInsertionConfiguration(allowedMimeTypes: pastedImageMimeTypes, onContentInserted: _insert) : null,
                decoration: InputDecoration(labelText: widget.required ? widget.label : '${widget.label} (optional)'),
              ),
              if (_uploading) const Padding(padding: EdgeInsets.only(top: 8), child: LinearProgressIndicator()),
              for (final f in _files)
                ListTile(
                  key: ValueKey('ticket-text-file-${f.path}'),
                  contentPadding: EdgeInsets.zero,
                  dense: true,
                  leading: const Icon(Icons.image_outlined),
                  title: Text(f.name, overflow: TextOverflow.ellipsis),
                  trailing: IconButton(tooltip: 'Remove', icon: const Icon(Icons.close, size: 18), onPressed: () => setState(() => _files.remove(f))),
                ),
            ]),
          ),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
          FilledButton(
            key: const ValueKey('ticket-text-save'),
            onPressed: valid ? () => Navigator.pop(context, (text: _text.text.trim(), files: [for (final f in _files) f.path])) : null,
            child: const Text('Save'),
          ),
        ],
      ),
    );
  }
}
