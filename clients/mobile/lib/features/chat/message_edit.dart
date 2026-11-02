import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:intl/intl.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/discussion.dart';
import '../../data/secret_write.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import 'secret_check.dart';

// Edit and delete a message of your own (A6, freeze §5): in a group channel and
// a ticket's discussion, behind `message_edits`. The server says per reader
// whether a message is `editable`; the phone never works it out itself.

/// What a deleted message says in its place (freeze §5).
const messageDeletedText = 'This message was deleted.';

/// Under the edit field (B3).
const messageEditHint = "Editing wakes no one. An employee that already read this keeps the original. This doesn't erase a secret: rotate it and put it in the vault.";

/// The question and the words under it in the delete dialog (B3).
const messageDeleteTitle = 'Delete this message?';
const messageDeleteBody = "A line stays that says it was deleted. The text stays in the audit log and backups, and employees that read it keep it. This doesn't erase a secret: rotate it and put it in the vault.";

/// Under the field when the new text names someone the old one did not.
String editMentionNote(List<String> names) => "${names.map((n) => '@$n').join(', ')} won't be notified by an edit. Send a new message to reach them.";

/// The `@names` in [after] that [before] did not have: an edit wakes no one.
List<String> newMentions(String before, String after) {
  final had = mentionedNames(before).map((n) => n.toLowerCase()).toSet();
  return [
    for (final n in mentionedNames(after).toSet())
      if (!had.contains(n.toLowerCase())) n,
  ];
}

/// Whether [message] offers Edit and Delete to the reader: the server has A6 and says it is editable.
bool messageEditable(BuildContext context, MessageView? message) =>
    message != null && message.editable == true && !message.isDeleted && context.read<AureliusRepository>().linkState.has(messageEditsFeature);

/// Whether [message] shows "edited" and its earlier texts: the server has A6.
bool messageEditedShown(BuildContext context, MessageView? message) =>
    message != null && message.editedAtMs != null && !message.isDeleted && context.read<AureliusRepository>().linkState.has(messageEditsFeature);

/// "Nov 2, 10:14" for the moment of an edit.
String editedAt(int ms) => DateFormat('MMM d, HH:mm').format(DateTime.fromMillisecondsSinceEpoch(ms));

/// Asks, then deletes [message]; it stays as a line that says so. No Undo.
Future<void> deleteOwnMessage(BuildContext context, MessageView message) async {
  final repository = context.read<AureliusRepository>();
  final sure = await showDialog<bool>(
    context: context,
    builder: (dialog) => AlertDialog(
      key: const ValueKey('message-delete-dialog'),
      title: const Text(messageDeleteTitle),
      content: const Text(messageDeleteBody),
      actions: [
        TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
        TextButton(key: const ValueKey('message-delete-confirm'), onPressed: () => Navigator.pop(dialog, true), style: TextButton.styleFrom(foregroundColor: Tokens.negative), child: const Text('Delete')),
      ],
    ),
  );
  if (sure != true || !context.mounted) return;
  try {
    await repository.write(Commands.deleteMessage(message.id));
  } on ProtocolException catch (e) {
    if (context.mounted) toast(context, e.shown);
  } on Object catch (e) {
    if (context.mounted) toast(context, '$e');
  }
}

/// The edit sheet for [message]: its text, the hint, Cancel and Save. Nothing
/// changed: Save closes without writing. Empty: Save is off. A failed save
/// keeps the sheet open with the reason.
Future<void> editOwnMessage(BuildContext context, MessageView message) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      useRootNavigator: true,
      builder: (sheet) => MessageEditSheet(message: message, repository: context.read<AureliusRepository>(), askContext: context),
    );

class MessageEditSheet extends StatefulWidget {
  const MessageEditSheet({super.key, required this.message, required this.repository, required this.askContext});

  final MessageView message;
  final AureliusRepository repository;

  /// Where the "looks like a vault secret" dialog opens from.
  final BuildContext askContext;

  @override
  State<MessageEditSheet> createState() => _MessageEditSheetState();
}

class _MessageEditSheetState extends State<MessageEditSheet> {
  late final _text = TextEditingController(text: widget.message.text);
  bool _saving = false;
  String? _failure;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final text = _text.text;
    if (text == widget.message.text) {
      Navigator.pop(context);
      return;
    }
    setState(() {
      _saving = true;
      _failure = null;
    });
    try {
      // B10: a text holding a vault value asks first; Send anyway writes it again with `confirm_secret`.
      final sent = await writeConfirmingSecret(widget.repository, Commands.editMessage(widget.message.id, text), sendAnywayDialog(widget.askContext));
      if (!mounted) return;
      if (sent != null) {
        Navigator.pop(context);
        return;
      }
      setState(() => _saving = false);
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _failure = e.shown;
      });
    } on Object catch (e) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _failure = '$e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.fromLTRB(16, 0, 16, MediaQuery.viewInsetsOf(context).bottom + 16),
      child: ListenableBuilder(
        listenable: _text,
        builder: (context, _) {
          final empty = _text.text.trim().isEmpty;
          final mentions = newMentions(widget.message.text, _text.text);
          return Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            const Text('Edit message', style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600)),
            const SizedBox(height: 12),
            TextField(key: const ValueKey('message-edit-field'), controller: _text, autofocus: true, minLines: 2, maxLines: 8, enabled: !_saving, textCapitalization: TextCapitalization.sentences),
            const SizedBox(height: 8),
            Text(messageEditHint, key: const ValueKey('message-edit-hint'), style: TextStyle(fontSize: 12.5, color: Tokens.label2)),
            if (mentions.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 6), child: Text(editMentionNote(mentions), key: const ValueKey('message-edit-mentions'), style: TextStyle(fontSize: 12.5, color: Tokens.warning))),
            if (empty) Padding(padding: const EdgeInsets.only(top: 6), child: Text('To remove it, delete the message.', style: TextStyle(fontSize: 12.5, color: Tokens.label2))),
            if (_failure != null) Padding(padding: const EdgeInsets.only(top: 6), child: Text(_failure!, key: const ValueKey('message-edit-failure'), style: TextStyle(fontSize: 12.5, color: Tokens.negative))),
            const SizedBox(height: 8),
            Row(mainAxisAlignment: MainAxisAlignment.end, children: [
              TextButton(key: const ValueKey('message-edit-cancel'), onPressed: _saving ? null : () => Navigator.pop(context), child: const Text('Cancel')),
              const SizedBox(width: 8),
              FilledButton(key: const ValueKey('message-edit-save'), onPressed: empty || _saving ? null : _save, child: const Text('Save')),
            ]),
          ]);
        },
      ),
    );
  }
}

/// The earlier texts of an edited message, newest first; read-only, for any board member.
Future<void> showMessageVersions(BuildContext context, MessageView message) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      useRootNavigator: true,
      builder: (sheet) => _Versions(message: message, repository: context.read<AureliusRepository>()),
    );

class _Versions extends StatefulWidget {
  const _Versions({required this.message, required this.repository});

  final MessageView message;
  final AureliusRepository repository;

  @override
  State<_Versions> createState() => _VersionsState();
}

class _VersionsState extends State<_Versions> {
  late Future<MessageHistoryView> _history = widget.repository.messageEdits(widget.message.id);

  @override
  Widget build(BuildContext context) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.7),
          child: FutureBuilder<MessageHistoryView>(
            future: _history,
            builder: (context, snapshot) {
              if (snapshot.hasError) {
                return Padding(
                  padding: const EdgeInsets.all(24),
                  child: Column(mainAxisSize: MainAxisSize.min, children: [
                    const Text('Could not load the earlier versions.'),
                    const SizedBox(height: 8),
                    Text('${snapshot.error}', style: TextStyle(color: Tokens.label2)),
                    TextButton(key: const ValueKey('message-versions-retry'), onPressed: () => setState(() => _history = widget.repository.messageEdits(widget.message.id)), child: const Text('Try again')),
                  ]),
                );
              }
              final history = snapshot.data;
              if (history == null) return const Padding(padding: EdgeInsets.all(32), child: Center(child: CircularProgressIndicator()));
              return ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(16, 0, 16, 16), children: [
                const Text('Earlier versions', style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600)),
                const SizedBox(height: 8),
                if (history.versions.isEmpty) Text('There are no earlier versions.', style: TextStyle(color: Tokens.label2)),
                for (final v in history.versions)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text('Replaced ${editedAt(v.replacedAtMs)}', style: TextStyle(fontSize: 12, color: Tokens.label2)),
                      const SizedBox(height: 2),
                      SelectableText(v.text),
                    ]),
                  ),
              ]);
            },
          ),
        ),
      );
}

/// A message that was deleted: its place stays, its words do not; no copy, no files, no reactions.
class DeletedMessageLine extends StatelessWidget {
  const DeletedMessageLine({super.key, required this.message, this.author});

  final MessageView message;

  /// Who wrote it, for the screen reader.
  final String? author;

  @override
  Widget build(BuildContext context) => Semantics(
        label: 'Deleted message${author == null ? '' : ' by $author'} at ${DateFormat.Hm().format(DateTime.fromMillisecondsSinceEpoch(message.createdAtMs))}.',
        excludeSemantics: true,
        child: Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            Icon(Icons.do_not_disturb_on_outlined, size: 14, color: Tokens.label2),
            const SizedBox(width: 6),
            Flexible(child: Text(messageDeletedText, key: ValueKey('message-deleted-${message.id}'), style: TextStyle(fontSize: 14, fontStyle: FontStyle.italic, color: Tokens.label2))),
          ]),
        ),
      );
}

/// "edited" under a message; a tap shows the earlier texts.
class EditedMark extends StatelessWidget {
  const EditedMark({super.key, required this.message});

  final MessageView message;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: 'Edited ${editedAt(message.editedAtMs!)}. Show earlier versions.',
        excludeSemantics: true,
        child: InkWell(
          key: ValueKey('message-edited-${message.id}'),
          onTap: () => showMessageVersions(context, message),
          child: Padding(padding: const EdgeInsets.symmetric(vertical: 4), child: Text('edited', style: TextStyle(fontSize: 11, color: Tokens.label2))),
        ),
      );
}
