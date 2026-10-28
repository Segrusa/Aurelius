import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:image_picker/image_picker.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../chat/upload_files.dart';
import '../project/new_task_page.dart' show maxTaskFileBytes, maxTaskFiles;

/// What [IdeaPage] returns. `files`: upload paths of the files added now;
/// `removeFiles`: paths of the idea's own attachments taken off.
typedef IdeaEdit = ({String title, String? note, IdeaKind kind, List<String> files, List<String> removeFiles});

/// A file on the form: one the idea already has ([file]), or uploaded now
/// ([path] is the upload; [local] the photo on the phone, for its thumbnail).
class _Attached {
  const _Attached({required this.name, required this.byteLength, required this.path, this.file, this.local});

  final String name;
  final int byteLength;
  final String path;
  final MessageFileView? file;
  final File? local;
}

/// A new idea, or [idea] to edit, on a full screen: title, a note over several
/// lines, Feature or Bug and, when the server keeps them ([attachments]),
/// photos and files.
class IdeaPage extends StatefulWidget {
  const IdeaPage({super.key, this.idea, this.attachments = false});

  final IdeaView? idea;
  final bool attachments;

  static Future<IdeaEdit?> open(BuildContext context, {IdeaView? idea, bool attachments = false}) => Navigator.of(context).push<IdeaEdit>(
        MaterialPageRoute(fullscreenDialog: true, builder: (_) => IdeaPage(idea: idea, attachments: attachments)),
      );

  @override
  State<IdeaPage> createState() => _IdeaPageState();
}

class _IdeaPageState extends State<IdeaPage> {
  late final _title = TextEditingController(text: widget.idea?.title);
  late final _note = TextEditingController(text: widget.idea?.note);
  late IdeaKind _kind = widget.idea?.kind ?? IdeaKind.feature;
  late List<_Attached> _files = [
    for (final f in widget.idea?.attachments ?? const <MessageFileView>[]) _Attached(name: f.name, byteLength: f.byteLength, path: f.path, file: f),
  ];
  final _removed = <String>[];
  bool _uploading = false;

  bool get _valid => _title.text.trim().isNotEmpty && !_uploading;

  @override
  void dispose() {
    _title.dispose();
    _note.dispose();
    super.dispose();
  }

  /// Photo, library or file, uploaded to the PC at once and kept with the idea on save.
  Future<void> _attach() async {
    if (_files.length >= maxTaskFiles) return toast(context, 'At most $maxTaskFiles files per idea.');
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(leading: const Icon(Icons.photo_camera_outlined), title: const Text('Take a photo'), onTap: () => Navigator.pop(sheet, 'camera')),
            ListTile(leading: const Icon(Icons.photo_library_outlined), title: const Text('Photo library'), onTap: () => Navigator.pop(sheet, 'gallery')),
            ListTile(leading: const Icon(Icons.description_outlined), title: const Text('File'), onTap: () => Navigator.pop(sheet, 'file')),
          ],
        ),
      ),
    );
    if (choice == null || !mounted) return;
    File? file;
    String? name;
    if (choice == 'file') {
      final picked = (await FilePicker.pickFiles()).firstOrNull;
      if (picked?.path != null) (file, name) = (File(picked!.path!), picked.name);
    } else {
      final image = await ImagePicker().pickImage(source: choice == 'camera' ? ImageSource.camera : ImageSource.gallery, imageQuality: 85, maxWidth: 2400);
      if (image != null) (file, name) = (File(image.path), image.name);
    }
    if (file == null || name == null || !mounted) return;
    final repository = context.read<AureliusRepository>();
    setState(() => _uploading = true);
    try {
      if (await file.length() > maxTaskFileBytes) {
        if (mounted) toast(context, '$name is larger than 25 MB.');
        return;
      }
      final uploaded = await repository.upload(file, name);
      final local = isImageName(name) ? file : null;
      if (mounted) setState(() => _files = [..._files, _Attached(name: uploaded.name, byteLength: uploaded.byteLength, path: uploaded.path, local: local)]);
    } catch (e) {
      if (mounted) toast(context, e.toString());
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  void _removeFile(_Attached f) => setState(() {
        _files = _files.where((x) => x != f).toList();
        if (f.file != null) _removed.add(f.path);
      });

  void _save() {
    if (!_valid) return;
    final note = _note.text.trim();
    Navigator.pop<IdeaEdit>(context, (
      title: _title.text.trim(),
      note: note.isEmpty ? null : note,
      kind: _kind,
      files: [for (final f in _files) if (f.file == null) f.path],
      removeFiles: List.of(_removed),
    ));
  }

  @override
  Widget build(BuildContext context) {
    final editing = widget.idea != null;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): _save,
        const SingleActivator(LogicalKeyboardKey.enter, control: true): _save,
      },
      child: Scaffold(
        appBar: GlassAppBar(
          title: Text(editing ? 'Edit idea' : 'New idea'),
          leading: IconButton(tooltip: 'Cancel', icon: const Icon(Icons.close), onPressed: () => Navigator.pop(context)),
          actions: [
            Padding(
              padding: const EdgeInsets.only(right: 12),
              child: FilledButton(key: const Key('idea-save'), onPressed: _valid ? _save : null, child: Text(editing ? 'Save' : 'Add')),
            ),
          ],
        ),
        body: SafeArea(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
            keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
            children: [
              TextField(
                key: const Key('idea-title'),
                controller: _title,
                autofocus: !editing,
                textCapitalization: TextCapitalization.sentences,
                textInputAction: TextInputAction.next,
                decoration: const InputDecoration(labelText: 'Idea', hintText: 'What could be better?'),
                onChanged: (_) => setState(() {}),
              ),
              const SizedBox(height: 14),
              TextField(
                key: const Key('idea-note'),
                controller: _note,
                minLines: 4,
                maxLines: 14,
                keyboardType: TextInputType.multiline,
                textInputAction: TextInputAction.newline,
                textCapitalization: TextCapitalization.sentences,
                decoration: const InputDecoration(labelText: 'Note (optional)', hintText: 'Context, steps, links… over as many lines as you like', alignLabelWithHint: true),
              ),
              const Eyebrow('Kind', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
              SizedBox(
                width: double.infinity,
                child: SegmentedButton<IdeaKind>(
                  showSelectedIcon: false,
                  segments: const [
                    ButtonSegment(value: IdeaKind.feature, label: Text('Feature')),
                    ButtonSegment(value: IdeaKind.bug, label: Text('Bug')),
                  ],
                  selected: {_kind},
                  onSelectionChanged: (s) => setState(() => _kind = s.first),
                ),
              ),
              if (widget.attachments) ...[
                const Eyebrow('Attachments', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  crossAxisAlignment: WrapCrossAlignment.end,
                  children: [
                    for (final f in _files)
                      f.file != null
                          ? IdeaAttachment(file: f.file!, onRemove: () => _removeFile(f))
                          : _NewAttachment(name: f.name, byteLength: f.byteLength, local: f.local, onRemove: () => _removeFile(f)),
                    if (_uploading) const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)),
                    ActionChip(
                      key: const Key('idea-attach'),
                      avatar: const Icon(Icons.add, size: 18),
                      label: const Text('Photo or file'),
                      onPressed: _uploading ? null : _attach,
                    ),
                  ],
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// A file uploaded now and not yet on the idea: its chip, with the photo above it.
class _NewAttachment extends StatelessWidget {
  const _NewAttachment({required this.name, required this.byteLength, required this.local, required this.onRemove});

  final String name;
  final int byteLength;
  final File? local;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final photo = local;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (photo != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(6),
              child: Image.file(photo, width: 120, height: 80, fit: BoxFit.cover, errorBuilder: (_, _, _) => const SizedBox.shrink()),
            ),
          ),
        InputChip(
          avatar: Icon(photo != null ? Icons.image_outlined : Icons.description_outlined, size: 18, color: Tokens.accentText),
          label: Text('$name · ${Fmt.bytes(byteLength)}', overflow: TextOverflow.ellipsis),
          onDeleted: onRemove,
        ),
      ],
    );
  }
}

/// A file kept with an idea: a chip, with a thumbnail for an image; tap opens
/// it (an image in the app, anything else through the phone). With [onRemove]
/// the chip can be taken off.
class IdeaAttachment extends StatelessWidget {
  const IdeaAttachment({super.key, required this.file, this.onRemove});

  final MessageFileView file;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final image = isImageName(file.name);
    final fetchable = uploadFetchable(context, file);
    final chip = InputChip(
      avatar: Icon(image ? Icons.image_outlined : Icons.description_outlined, size: 18, color: Tokens.accentText),
      label: Text('${file.name} · ${Fmt.bytes(file.byteLength)}', overflow: TextOverflow.ellipsis),
      onPressed: fetchable ? () => openUpload(context, file) : null,
      onDeleted: onRemove,
    );
    return Column(
      key: Key('idea-attachment-${file.path}'),
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (image && fetchable)
          GestureDetector(
            onTap: () => openUpload(context, file),
            child: Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: UploadThumb(id: file.id!, width: 120, height: 80, fallback: const SizedBox.shrink()),
            ),
          ),
        chip,
      ],
    );
  }
}
