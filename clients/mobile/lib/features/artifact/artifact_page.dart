import 'dart:convert';
import 'dart:typed_data';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/models.dart';
import '../chat/chat_bloc.dart';
import 'artifact_bloc.dart';
import 'artifact_versions.dart';
import 'artifact_webview.dart';

/// Saves the shown version's HTML where the board member chooses; null when
/// they cancelled. Tests replace it: the platform's save dialog is not there.
Future<Uri?> Function(String name, Uint8List bytes) artifactSave =
    (name, bytes) => FilePicker.saveFile(fileName: name, bytes: bytes, mimeType: 'text/html');

/// Shows one load of an artifact; tests replace it (a platform webview cannot run there).
Widget Function(ArtifactState state) artifactStage =
    (state) => ArtifactWebView(document: state.document!, load: state.load, width: state.viewport.width);

/// An HTML file a run kept with its message, opened as a live design: its
/// versions in the conversation, three widths, Reload, Share HTML and Ask for
/// changes (one message in the conversation it came from).
class ArtifactPage extends StatelessWidget {
  const ArtifactPage({super.key, this.canAsk = false});

  /// The conversation takes messages: Ask for changes is offered.
  final bool canAsk;

  /// Opens [file] from a chat: its versions are those of the chat's
  /// conversation, and Ask for changes posts there, when the chat can be written to.
  static Future<void> open(BuildContext context, FileRef file, {bool canAsk = true}) {
    final repository = context.read<AureliusRepository>();
    ChatBloc? chat;
    try {
      chat = context.read<ChatBloc>();
    } on Exception {
      chat = null;
    }
    final now = DateTime.now().millisecondsSinceEpoch;
    final ask = canAsk && chat != null;
    final page = BlocProvider(
      create: (_) => ArtifactBloc(repository, file: file, messages: chat?.state.messages ?? const [], openedAtMs: now),
      child: chat == null ? ArtifactPage(canAsk: ask) : _FollowsChat(child: ArtifactPage(canAsk: ask)),
    );
    return Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => RepositoryProvider.value(value: repository, child: chat == null ? page : BlocProvider.value(value: chat, child: page)),
    ));
  }

  Future<void> _share(BuildContext context, ArtifactState state) async {
    final html = state.html;
    if (html == null) return;
    final name = state.current.file.name;
    try {
      final saved = await artifactSave(name, utf8.encode(html));
      if (saved != null && context.mounted) toast(context, 'Saved $name.');
    } on Exception catch (e) {
      if (context.mounted) toast(context, 'Could not save $name: $e');
    }
  }

  Future<void> _ask(BuildContext context, ArtifactState state) async {
    final v = state.current;
    final chat = context.read<ChatBloc>();
    final text = await promptText(context, title: 'Ask for changes', label: 'What should change in ${v.file.name} v${v.version}?', lines: 4, action: 'Send');
    if (text == null || text.trim().isEmpty) return;
    final trimmed = text.trim();
    chat.add(ChatMessageSent(artifactFeedback(v.file.name, v.version, trimmed.length > artifactFeedbackMax ? trimmed.substring(0, artifactFeedbackMax) : trimmed), withAttachments: false));
    if (context.mounted) toast(context, 'Sent to the conversation.');
  }

  @override
  Widget build(BuildContext context) {
    return BlocBuilder<ArtifactBloc, ArtifactState>(
      builder: (context, state) {
        final bloc = context.read<ArtifactBloc>();
        final v = state.current;
        return Scaffold(
          backgroundColor: Tokens.bg,
          appBar: GlassAppBar(
            title: Text(v.file.name, overflow: TextOverflow.ellipsis),
            actions: [
              IconButton(tooltip: 'Reload', icon: const Icon(CupertinoIcons.arrow_clockwise), onPressed: state.html == null ? null : () => bloc.add(const ArtifactReloaded())),
              IconButton(tooltip: 'Share HTML', icon: const Icon(CupertinoIcons.share), onPressed: state.html == null ? null : () => _share(context, state)),
            ],
          ),
          body: SafeArea(
            top: false,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                  child: Row(
                    children: [
                      _VersionPicker(state: state),
                      const SizedBox(width: 12),
                      Expanded(
                        child: PillSegments<ArtifactViewport>(
                          segments: [for (final w in ArtifactViewport.values) (w, w.label)],
                          selected: state.viewport,
                          onChanged: (w) => bloc.add(ArtifactViewportSelected(w)),
                        ),
                      ),
                    ],
                  ),
                ),
                Expanded(
                  child: Container(
                    key: const Key('artifact-stage'),
                    margin: const EdgeInsets.symmetric(horizontal: 12),
                    clipBehavior: Clip.antiAlias,
                    decoration: BoxDecoration(color: Tokens.cell, borderRadius: BorderRadius.circular(12), border: Border.all(color: Tokens.sep)),
                    child: _stage(context, state),
                  ),
                ),
                if (canAsk)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
                    child: FilledButton.icon(
                      onPressed: () => _ask(context, state),
                      icon: const Icon(CupertinoIcons.chat_bubble_text),
                      label: const Text('Ask for changes'),
                    ),
                  )
                else
                  const SizedBox(height: 12),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _stage(BuildContext context, ArtifactState state) {
    final error = state.error;
    if (error != null) {
      return ErrorRetry('This artifact could not be loaded: $error', onRetry: () => context.read<ArtifactBloc>().add(const ArtifactRetried()));
    }
    if (state.document == null) return const Center(child: CircularProgressIndicator());
    return artifactStage(state);
  }
}

/// The version picker: "v3" on the toolbar (a narrow phone keeps the whole
/// width toggle), "v3 · 14:05" newest first in its menu.
class _VersionPicker extends StatelessWidget {
  const _VersionPicker({required this.state});

  final ArtifactState state;

  @override
  Widget build(BuildContext context) {
    final now = DateTime.now();
    final only = state.versions.length < 2;
    return Builder(
      builder: (anchor) => TextButton.icon(
        key: const Key('artifact-version'),
        onPressed: only
            ? null
            : () async {
                final id = await showGlassMenu<String>(anchor, [
                  for (final v in state.versions) GlassMenuItem(v.file.id, versionLabel(v, now), selected: v.file.id == state.fileId),
                ]);
                if (id != null && anchor.mounted) anchor.read<ArtifactBloc>().add(ArtifactVersionSelected(id));
              },
        iconAlignment: IconAlignment.end,
        icon: only ? const SizedBox.shrink() : const Icon(CupertinoIcons.chevron_down, size: 14),
        label: Text('v${state.current.version}', semanticsLabel: 'Version ${state.current.version}', style: const TextStyle(fontWeight: FontWeight.w600)),
      ),
    );
  }
}

/// Keeps the artifact's versions up to date while it is open: an employee's
/// next version shows up in the picker.
class _FollowsChat extends StatelessWidget {
  const _FollowsChat({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => BlocListener<ChatBloc, ChatState>(
        listenWhen: (a, b) => a.messages != b.messages,
        listener: (context, chat) => context.read<ArtifactBloc>().add(ArtifactMessagesChanged(chat.messages)),
        child: child,
      );
}
