import 'dart:async';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:go_router/go_router.dart';
import 'package:image_picker/image_picker.dart';

import '../../core/clipboard/image_clipboard.dart';
import '../../core/net/compacting.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../../data/markdown_paths.dart';
import '../../data/run_config.dart';
import '../runtime/runtime_bloc.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../company_org/employee_run_sheet.dart';
import '../inbox/inbox_bloc.dart';
import '../load/load_widgets.dart';
import 'answer_body.dart';
import 'chat_bloc.dart';
import 'collapse.dart';
import 'file_page.dart';
import '../markdown/links.dart';
import '../markdown/markdown_page.dart';
import 'following_list.dart';
import 'live_steps.dart';
import 'git_chip.dart';
import 'subagent_chip.dart';
import 'upload_files.dart';

MarkdownStyleSheet markdownStyle(BuildContext context, {double size = 15}) {
  final base = MarkdownStyleSheet.fromTheme(Theme.of(context));
  return base.copyWith(
    p: TextStyle(fontSize: size, height: 1.7, color: Tokens.text),
    h1: Tokens.title(size: 24),
    h2: Tokens.title(size: 20),
    h3: TextStyle(fontSize: 16, fontWeight: FontWeight.w600, color: Tokens.text),
    code: TextStyle(fontFamily: Tokens.mono, fontSize: size - 2, color: Tokens.accentText, backgroundColor: Tokens.surface2),
    codeblockDecoration: BoxDecoration(color: Tokens.surface, borderRadius: BorderRadius.circular(Tokens.radius), border: Border.all(color: Tokens.border)),
    blockquoteDecoration: BoxDecoration(border: Border(left: BorderSide(color: Tokens.borderStrong, width: 3))),
    blockquote: TextStyle(color: Tokens.text2),
    a: TextStyle(color: Tokens.accentText),
    listBullet: TextStyle(fontSize: size, color: Tokens.text2),
  );
}

/// The Markdown scope of the conversation a message is in (`null` while a new
/// chat has no conversation yet).
FileScope? chatFileScope(BuildContext context) {
  try {
    final id = context.read<ChatBloc>().state.conversationId;
    return id == null ? null : FileScopeConversation(conversationId: id);
  } on ProviderNotFoundException {
    return null;
  }
}

/// Messages and composer of one conversation (chat or employee).
class ChatView extends StatefulWidget {
  const ChatView({super.key, this.emptyTitle = 'What can I do for you?', this.emptyDetail, this.composer = true});

  final String emptyTitle;
  final String? emptyDetail;

  /// `false` for a conversation that cannot be written to.
  final bool composer;

  @override
  State<ChatView> createState() => _ChatViewState();
}

class _ChatViewState extends State<ChatView> {
  /// What is typed in the composer; quick replies give way to it.
  final _draft = ValueNotifier<String>('');

  @override
  void dispose() {
    _draft.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final emptyTitle = widget.emptyTitle;
    final emptyDetail = widget.emptyDetail;
    final composer = widget.composer;
    final bloc = context.read<ChatBloc>();
    return BlocListener<ChatBloc, ChatState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<ChatBloc>().add(const ChatNoticeShown());
      },
      child: BlocListener<RuntimeBloc, RuntimeState>(
        listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
        listener: (context, state) {
          toast(context, state.notice!);
          context.read<RuntimeBloc>().add(const RuntimeNoticeShown());
        },
        child: Column(
          children: [
            Expanded(child: _Messages(emptyTitle: emptyTitle, emptyDetail: emptyDetail)),
            BlocSelector<ChatBloc, ChatState, bool>(
              selector: (s) => s.awaitingStart,
              builder: (context, waiting) => LoadBanner(waiting: waiting),
            ),
            if (bloc.state.conversationId != null)
              Padding(
                padding: const EdgeInsets.only(top: 6, bottom: 10),
                child: Row(
                children: [
                  Expanded(child: GitChip(key: ValueKey(bloc.state.conversationId), conversationId: bloc.state.conversationId!)),
                  BlocSelector<ChatBloc, ChatState, bool>(
                    selector: (s) => s.streaming != null,
                    builder: (context, running) => SubagentChip(key: ValueKey('subagents-${bloc.state.conversationId}'), conversationId: bloc.state.conversationId!, running: running, employee: true),
                  ),
                ],
              ),
              ),
            if (composer) _QuickRepliesCard(draft: _draft),
            if (composer) _Composer(draft: _draft),
          ],
        ),
      ),
    );
  }
}

/// Suggested answers under the last complete answer (server feature
/// `quick_replies`): one tap sends one as a normal message.
class _QuickRepliesCard extends StatefulWidget {
  const _QuickRepliesCard({required this.draft});

  final ValueNotifier<String> draft;

  @override
  State<_QuickRepliesCard> createState() => _QuickRepliesCardState();
}

class _QuickRepliesCardState extends State<_QuickRepliesCard> {
  /// The answer whose suggestions were dismissed with the x.
  String? _dismissed;

  @override
  Widget build(BuildContext context) {
    final enabled = context.select<ConnectionBloc, bool>((b) => b.state.has(quickRepliesFeature));
    if (!enabled) return const SizedBox.shrink();
    final state = context.watch<ChatBloc>().state;
    final blocked = context.select<InboxBloc, ApprovalView?>((b) => b.state.blocking(state.conversationId)) != null;
    return ValueListenableBuilder<String>(
      valueListenable: widget.draft,
      builder: (context, draft, _) {
        final replies = quickRepliesToShow(
          messages: state.messages,
          enabled: enabled,
          hasUnsent: state.unsent.isNotEmpty,
          blocked: blocked,
          draft: draft,
          dismissedId: _dismissed,
        );
        if (replies == null) return const SizedBox.shrink();
        return Padding(
          key: const Key('quick-replies'),
          padding: const EdgeInsets.only(bottom: 8),
          child: Material(
            color: Tokens.cell,
            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius), side: BorderSide(color: Tokens.accent.withValues(alpha: 0.45))),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 4, 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      Expanded(child: replies.question.isEmpty ? const SizedBox.shrink() : Text(replies.question, style: TextStyle(color: Tokens.text2, fontSize: 14))),
                      IconButton(
                        key: const Key('quick-replies-dismiss'),
                        tooltip: 'Dismiss suggestions',
                        constraints: const BoxConstraints(minWidth: 44, minHeight: 44),
                        icon: Icon(Icons.close, size: 18, color: Tokens.muted),
                        onPressed: () => setState(() => _dismissed = replies.messageId),
                      ),
                    ],
                  ),
                  Padding(
                    padding: const EdgeInsets.only(right: 12),
                    child: Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        for (final (i, option) in replies.options.indexed)
                          ConstrainedBox(
                            constraints: const BoxConstraints(minHeight: 44),
                            child: ActionChip(
                              key: Key('quick-reply-$i'),
                              backgroundColor: i == 0 ? Tokens.accent : null,
                              label: Text(quickReplyLabel(option), style: TextStyle(color: i == 0 ? Colors.white : Tokens.text2, fontWeight: i == 0 ? FontWeight.w600 : null)),
                              onPressed: () => context.read<ChatBloc>().add(ChatMessageSent(option)),
                            ),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class _Messages extends StatefulWidget {
  const _Messages({required this.emptyTitle, this.emptyDetail});

  final String emptyTitle;
  final String? emptyDetail;

  @override
  State<_Messages> createState() => _MessagesState();
}

class _MessagesState extends State<_Messages> {
  final _list = GlobalKey<FollowingListState>();

  @override
  Widget build(BuildContext context) {
    final state = context.watch<ChatBloc>().state;
    final blocking = context.select<InboxBloc, ApprovalView?>((b) => b.state.blocking(state.conversationId));
    // Sending a message of your own goes back to the end and follows again.
    return BlocListener<ChatBloc, ChatState>(
      listenWhen: (a, b) => b.unsent.length > a.unsent.length,
      listener: (context, _) => WidgetsBinding.instance.addPostFrameCallback((_) => _list.currentState?.scrollToEnd()),
      child: _build(state, blocking),
    );
  }

  Widget _build(ChatState state, ApprovalView? blocking) {
    if (state.loading && state.messages.isEmpty) return const Center(child: CircularProgressIndicator());
    if (state.messages.isEmpty && state.unsent.isEmpty && state.loadError != null) {
      return EmptyState(
        'This conversation could not be loaded',
        key: const Key('chat-load-error'),
        detail: state.loadError,
        action: OutlinedButton(onPressed: () => context.read<ChatBloc>().add(const ChatRefreshed()), child: const Text('Try again')),
      );
    }
    if (state.messages.isEmpty && state.unsent.isEmpty) return EmptyState(widget.emptyTitle, detail: widget.emptyDetail);

    final versions = _versions(state.messages);
    final live = state.messages.indexWhere((m) => m.isStreaming);
    final shared = state.shares.isNotEmpty ? 1 : 0;
    final items = <FollowItem>[
      if (state.shares.isNotEmpty) FollowItem('shares', _SharedContext(shares: state.shares)),
      for (final m in state.messages)
        FollowItem(
          m.id,
          m.isUser
              ? _UserBubble(id: m.id, text: m.text, files: m.files, uploads: m.uploads, queuedId: m.queued ? m.id : null)
              : m.update != null
                  ? _TaskUpdate(message: m)
                  : _AssistantMessage(message: m, version: versions[m.id], blocking: m.isStreaming ? blocking : null),
        ),
      for (final u in state.unsent) FollowItem('unsent-${u.localId}', _UserBubble(id: 'unsent-${u.localId}', text: u.text, unsent: u)),
    ];
    // Newest at the bottom; the list starts there and holds still when you
    // scroll up while text comes in.
    return FollowingList(
      key: _list,
      items: items,
      liveFrom: live < 0 ? shared + state.messages.length : shared + live,
      resetKey: state.conversationId,
    );
  }

  /// "Version 2 of 3" for retried answers to the same user message.
  static Map<String, (int, int)> _versions(List<MessageView> messages) {
    final groups = <String, List<String>>{};
    String? lastUser;
    for (final m in messages) {
      if (m.isUser) {
        lastUser = m.id;
        continue;
      }
      final key = m.replyTo ?? lastUser ?? m.id;
      groups.putIfAbsent(key, () => []).add(m.id);
    }
    return {
      for (final ids in groups.values)
        if (ids.length > 1)
          for (final (i, id) in ids.indexed) id: (i + 1, ids.length),
    };
  }
}

/// A task update: the task itself, or what a worker, subagent or colleague
/// reports back to the employee.
class _TaskUpdate extends StatelessWidget {
  const _TaskUpdate({required this.message});

  final MessageView message;

  @override
  Widget build(BuildContext context) {
    // Something a worker asks: decided from here while it is pending.
    final pending = context.select<InboxBloc, ApprovalView?>((b) => message.approvalId == null ? null : b.state.approval(message.approvalId!));
    final (label, accent) = switch (message.update) {
      'brief' => ('TASK', true),
      'finding' => ('FINDING', false),
      'question' => ('NEEDS A DECISION', true),
      'approval' when pending?.isQuestion ?? false => ('QUESTION · WAITING FOR YOU', true),
      'approval' when pending != null => ('WAITING FOR YOU', true),
      'approval' => ('ASKED', false),
      'plan' when pending != null => ('PLAN · WAITING FOR YOU', true),
      'plan' => ('PLAN', false),
      // message_task: the delegating employee writes to this task.
      'message' => ('MESSAGE FROM', false),
      // set_mode: a management employee chose Auto or Plan (text: "<name> chose Auto: <reason>").
      'mode_chosen' => ('MODE CHOSEN', false),
      _ => ('UPDATE', false),
    };
    return Padding(
      padding: const EdgeInsets.only(top: 14),
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
        decoration: BoxDecoration(
          color: accent ? Tokens.attentionBg : Tokens.surface,
          border: Border.all(color: accent ? Tokens.attentionBorder : Tokens.border),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('$label${message.update == 'message' ? ' ' : ' · '}${message.author ?? 'Aurelius'}',style: Tokens.eyebrow().copyWith(fontSize: 10.5)),
            const SizedBox(height: 6),
            Collapsible(
              id: message.id,
              text: message.text,
              builder: (context, cut) => MessageMarkdown(data: cut == null ? message.text : collapsedMarkdown(message.text, cut), scope: chatFileScope(context), selectable: true),
            ),
            if (message.files.isNotEmpty) MessageFiles(files: message.files),
            if (message.text.isNotEmpty) Align(alignment: Alignment.centerRight, child: CopyButton(key: const Key('copy-update'), text: message.text)),
            if (pending != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: FilledButton.icon(
                  onPressed: () => context.push('/approval/${pending.id}'),
                  icon: Icon(pending.isQuestion ? Icons.help_outline : Icons.pan_tool_outlined, size: 18),
                  label: Text(pending.isQuestion ? 'Answer' : 'Decide'),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _UserBubble extends StatelessWidget {
  const _UserBubble({required this.id, required this.text, this.files = const [], this.uploads = const [], this.unsent, this.queuedId});

  /// Keeps it unfolded or folded while the list scrolls.
  final String id;
  final String text;

  /// Images sent with it (the text may be empty then).
  final List<FileRef> files;

  /// Files sent with it to an employee.
  final List<MessageFileView> uploads;
  final UnsentMessage? unsent;

  /// Set while the message waits behind a running answer on the PC.
  final String? queuedId;

  void _longPress(BuildContext context) async {
    if (queuedId == null) {
      Clipboard.setData(ClipboardData(text: text));
      toast(context, 'Copied.');
      return;
    }
    final bloc = context.read<ChatBloc>();
    final choice = await showGlassMenu<String>(context, const [
      GlassMenuItem('now', 'Send now'),
      GlassMenuItem('copy', 'Copy'),
      GlassMenuItem('delete', 'Delete', danger: true),
    ]);
    switch (choice) {
      case 'now':
        bloc.add(ChatQueuedSentNow(queuedId!));
      case 'delete':
        bloc.add(ChatQueuedDeleted(queuedId!));
      case 'copy':
        Clipboard.setData(ClipboardData(text: text));
        if (context.mounted) toast(context, 'Copied.');
    }
  }

  @override
  Widget build(BuildContext context) {
    final pending = unsent != null && !unsent!.failed && !unsent!.delivered;
    final muted = pending || queuedId != null;
    final sentFiles = [
      for (final u in uploads) (u.name, u.byteLength, u.path, u),
      for (final f in unsent?.files ?? const <PendingFile>[]) (f.name, f.byteLength, null, null),
    ];
    return Padding(
      padding: const EdgeInsets.only(top: 14, left: 48),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          if (text.isNotEmpty || (files.isEmpty && sentFiles.isEmpty && unsent?.images.isEmpty != false)) Collapsible(
            id: id,
            text: text,
            alignment: CrossAxisAlignment.end,
            // Long-press copies the whole message, also while it is folded.
            builder: (context, cut) => GestureDetector(
              onLongPress: () => _longPress(context),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                decoration: BoxDecoration(
                  color: queuedId != null ? Tokens.cell2.withValues(alpha: 0.55) : Tokens.cell2,
                  borderRadius: const BorderRadius.only(topLeft: Radius.circular(20), topRight: Radius.circular(20), bottomLeft: Radius.circular(20), bottomRight: Radius.circular(6)),
                ),
                child: LinkedText(cut == null ? text : text.substring(0, cut), scope: chatFileScope(context), style: TextStyle(fontSize: 16, height: 1.4, color: muted ? Tokens.label2 : Tokens.label)),
              ),
            ),
          ),
          if (text.isNotEmpty) CopyButton(key: const Key('copy-user'), text: text),
          if (files.isNotEmpty) MessageFiles(files: files),
          if (sentFiles.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                alignment: WrapAlignment.end,
                children: [for (final (name, size, path, file) in sentFiles) _UploadChip(name: name, byteLength: size, path: path, file: file)],
              ),
            ),
          if (unsent != null && unsent!.images.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Wrap(spacing: 6, runSpacing: 6, alignment: WrapAlignment.end, children: [for (final i in unsent!.images) _Thumbnail(file: i.local)]),
            ),
          if (pending)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text('Sending…', style: TextStyle(fontSize: 12, color: Tokens.muted)),
                  const SizedBox(width: 8),
                  _GlassPill(key: const Key('unsent-discard'), label: 'Discard', onTap: () => context.read<ChatBloc>().add(ChatUnsentDiscarded(unsent!.localId))),
                ],
              ),
            ),
          if (queuedId != null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                alignment: WrapAlignment.end,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Padding(
                    padding: const EdgeInsets.only(right: 2),
                    child: Text('Waiting · sends after the current answer', style: TextStyle(fontSize: 12, color: Tokens.muted)),
                  ),
                  _GlassPill(key: const Key('queued-send-now'), label: 'Send now', accent: true, onTap: () => context.read<ChatBloc>().add(ChatQueuedSentNow(queuedId!))),
                  _GlassPill(key: const Key('queued-delete'), label: 'Delete', danger: true, onTap: () => context.read<ChatBloc>().add(ChatQueuedDeleted(queuedId!))),
                ],
              ),
            ),
          if (unsent?.failed == true)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.error_outline, size: 14, color: Tokens.negative),
                  const SizedBox(width: 4),
                  Text('Not sent', style: TextStyle(fontSize: 12, color: Tokens.negative)),
                  TextButton(onPressed: () => context.read<ChatBloc>().add(ChatUnsentRetried(unsent!.localId)), child: const Text('Try again')),
                  TextButton(
                    onPressed: () => context.read<ChatBloc>().add(ChatUnsentDiscarded(unsent!.localId)),
                    style: TextButton.styleFrom(foregroundColor: Tokens.muted),
                    child: const Text('Discard'),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _AssistantMessage extends StatelessWidget {
  const _AssistantMessage({required this.message, this.version, this.blocking});

  final MessageView message;
  final (int, int)? version;
  final ApprovalView? blocking;

  String? _replyTarget(ChatState state) {
    if (message.replyTo != null) return message.replyTo;
    final i = state.messages.indexWhere((m) => m.id == message.id);
    for (var j = i - 1; j >= 0; j--) {
      if (state.messages[j].isUser) return state.messages[j].id;
    }
    return null;
  }

  void _actions(BuildContext context) {
    final bloc = context.read<ChatBloc>();
    final target = _replyTarget(bloc.state);
    showModalBottomSheet<void>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.copy),
              title: const Text('Copy'),
              onTap: () {
                Clipboard.setData(ClipboardData(text: message.text));
                Navigator.pop(sheet);
              },
            ),
            if (target != null && !message.isStreaming)
              ListTile(
                leading: const Icon(Icons.refresh),
                title: const Text('Answer again'),
                onTap: () {
                  bloc.add(ChatReplyRetried(target));
                  Navigator.pop(sheet);
                },
              ),
            if (bloc.state.conversationId != null && !message.isStreaming)
              ListTile(
                leading: const Icon(Icons.ios_share),
                title: const Text('Share to a project or employee'),
                onTap: () {
                  Navigator.pop(sheet);
                  showShareSheet(context, bloc, message.id);
                },
              ),
            if (message.runId != null)
              ListTile(
                leading: const Icon(Icons.manage_search),
                title: const Text('What went into this answer'),
                onTap: () {
                  Navigator.pop(sheet);
                  context.push('/context/${message.runId}');
                },
              ),
            for (final s in message.sources)
              ListTile(
                dense: true,
                leading: Text('[${s.n}]', style: Tokens.monoSmall()),
                title: Text(s.title ?? s.url, maxLines: 1, overflow: TextOverflow.ellipsis),
                subtitle: SelectableText(s.error ?? s.url, style: TextStyle(fontSize: 12, color: Tokens.muted)),
              ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final header = [
      if (message.author != null) message.author!,
      if (message.producedBy != null) message.producedBy!,
      if (version != null) 'version ${version!.$1}/${version!.$2}',
    ].join(' · ');
    return Padding(
      padding: const EdgeInsets.only(top: 18),
      child: GestureDetector(
        onLongPress: () => _actions(context),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (header.isNotEmpty) Padding(padding: const EdgeInsets.only(bottom: 4), child: Text(header, style: Tokens.monoSmall())),
            if (message.text.isNotEmpty || message.activity.isNotEmpty)
              Collapsible(id: message.id, text: message.text, streaming: message.isStreaming, builder: (context, cut) => AnswerBody(message: message, cut: cut)),
            if (message.files.isNotEmpty) MessageFiles(files: message.files),
            if (message.sources.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    for (final s in message.sources)
                      Chip(
                        visualDensity: VisualDensity.compact,
                        label: Text('[${s.n}] ${s.title ?? Uri.tryParse(s.url)?.host ?? s.url}', maxLines: 1, overflow: TextOverflow.ellipsis),
                      ),
                  ],
                ),
              ),
            if (message.text.isNotEmpty && !message.isStreaming) CopyButton(key: const Key('copy-assistant'), text: message.text),
            _StatusLine(message: message, blocking: blocking, retryTarget: _replyTarget(context.read<ChatBloc>().state)),
          ],
        ),
      ),
    );
  }
}

/// " · 2m 26s · 3.9k tokens" next to Thinking/Writing; ticks every second.
class _LiveProgress extends StatefulWidget {
  const _LiveProgress({required this.messageId});

  final String messageId;

  @override
  State<_LiveProgress> createState() => _LiveProgressState();
}

class _LiveProgressState extends State<_LiveProgress> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = context.select((ChatBloc b) => b.state.progress[widget.messageId]);
    final step = context.select((ChatBloc b) => currentStep(b.state.messages.where((m) => m.id == widget.messageId).firstOrNull?.activity ?? const []));
    if (p == null) return const SizedBox.shrink();
    return Flexible(
      child: Padding(
        padding: const EdgeInsets.only(left: 8),
        child: Text(
          [if (p.compacting == true) compactingLabel else if (step != null) step, Fmt.progress(p.startedAtMs, p.outputTokens)].where((t) => t.isNotEmpty).join(' · '),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(color: Tokens.muted, fontSize: 12.5),
        ),
      ),
    );
  }
}

class _StatusLine extends StatelessWidget {
  const _StatusLine({required this.message, this.blocking, this.retryTarget});

  final MessageView message;
  final ApprovalView? blocking;
  final String? retryTarget;

  @override
  Widget build(BuildContext context) {
    final bloc = context.read<ChatBloc>();
    switch (message.status) {
      case MessageStatus.partial:
        if (blocking != null) {
          return AttentionCard(
            onTap: () => context.push('/approval/${blocking!.id}'),
            child: Row(
              children: [
                Icon(blocking!.isQuestion ? Icons.help_outline : Icons.pan_tool_outlined, size: 18, color: Tokens.accentText),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    blocking!.isQuestion ? 'Waiting for your answer' : 'Waiting for your approval: ${blocking!.title}',
                    style: TextStyle(color: Tokens.accentText),
                  ),
                ),
                Icon(Icons.chevron_right, color: Tokens.accentText),
              ],
            ),
          );
        }
        return Row(
          children: [
            const StatusDot(pulse: true),
            const SizedBox(width: 8),
            Text(message.text.isEmpty && message.activity.isEmpty ? 'Thinking…' : 'Writing…', style: TextStyle(color: Tokens.text2, fontSize: 13)),
            _LiveProgress(messageId: message.id),
            const Spacer(),
            if (message.runId != null)
              IconButton(key: const Key('stop-run'), tooltip: 'Stop', visualDensity: VisualDensity.compact, onPressed: () => bloc.add(ChatStopRequested(message.runId!)), icon: const Icon(Icons.stop_circle_outlined, size: 20)),
          ],
        );
      case MessageStatus.interrupted || MessageStatus.failed:
        final failed = message.status == MessageStatus.failed;
        return Row(
          children: [
            Icon(failed ? Icons.error_outline : Icons.stop_circle_outlined, size: 15, color: failed ? Tokens.negative : Tokens.muted),
            const SizedBox(width: 6),
            Text(failed ? 'Failed' : 'Stopped', style: TextStyle(fontSize: 13, color: failed ? Tokens.negative : Tokens.muted)),
            if (retryTarget != null) IconButton(key: const Key('retry-answer'), tooltip: 'Answer again', visualDensity: VisualDensity.compact, onPressed: () => bloc.add(ChatReplyRetried(retryTarget!)), icon: const Icon(Icons.refresh, size: 20)),
          ],
        );
      case MessageStatus.complete:
        return const SizedBox(height: 2);
    }
  }
}

class _SharedContext extends StatelessWidget {
  const _SharedContext({required this.shares});

  final List<ShareView> shares;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(top: 8),
        child: SurfaceCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('SHARED CONTEXT', style: Tokens.eyebrow()),
              for (final s in shares) ...[
                const SizedBox(height: 8),
                Text(s.title, style: const TextStyle(fontWeight: FontWeight.w600)),
                Text(s.excerpt, maxLines: 3, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.text2, fontSize: 13)),
              ],
            ],
          ),
        ),
      );
}

/// Share one answer into a project or an employee, as context or as a memory proposal.
void showShareSheet(BuildContext context, ChatBloc bloc, String messageId) {
  final company = context.read<CompanyBloc>().state;
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) {
      var mode = ShareMode.context;
      return StatefulBuilder(
        builder: (context, setState) => SafeArea(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.75),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: SegmentedButton<ShareMode>(
                    segments: const [
                      ButtonSegment(value: ShareMode.context, label: Text('As context')),
                      ButtonSegment(value: ShareMode.memory, label: Text('As memory proposal')),
                    ],
                    selected: {mode},
                    onSelectionChanged: (s) => setState(() => mode = s.first),
                  ),
                ),
                Flexible(
                  child: ListView(
                    shrinkWrap: true,
                    children: [
                      if (company.visibleProjects.isNotEmpty) const Padding(padding: EdgeInsets.fromLTRB(16, 12, 16, 4), child: Eyebrow('Projects')),
                      for (final p in company.visibleProjects)
                        ListTile(
                          leading: const Icon(Icons.folder_outlined),
                          title: Text(p.name),
                          onTap: () {
                            bloc.add(ChatShared(messageId, targetKind: 'project', targetId: p.id, mode: mode));
                            Navigator.pop(sheet);
                          },
                        ),
                      for (final g in pickerGroups(company.activeEmployees, company.activeDepartments)) ...[
                        Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 4), child: Eyebrow(g.title)),
                        for (final m in g.employees)
                          ListTile(
                            leading: Monogram(m.monogram, size: 26),
                            title: Text(m.name),
                            subtitle: m.roleLine.isEmpty ? null : Text(m.roleLine, style: TextStyle(color: Tokens.muted, fontSize: 12)),
                            onTap: () {
                              bloc.add(ChatShared(messageId, targetKind: 'employee', targetId: m.id, mode: mode));
                              Navigator.pop(sheet);
                            },
                          ),
                      ],
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}

class _Composer extends StatefulWidget {
  const _Composer({this.draft});

  /// Told what is typed, so quick replies can step aside.
  final ValueNotifier<String>? draft;

  @override
  State<_Composer> createState() => _ComposerState();
}

class _ComposerState extends State<_Composer> with WidgetsBindingObserver {
  final _text = TextEditingController();
  final _focus = FocusNode();
  final _clipboard = ImageClipboard();

  /// Whether the clipboard holds an image, for the Paste Image menu item.
  /// Checked when the field is focused or tapped and when the app returns,
  /// since the edit menu is built synchronously.
  bool _clipboardImage = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _focus.addListener(() {
      if (_focus.hasFocus) _checkClipboard();
    });
    _text.addListener(() => widget.draft?.value = _text.text);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _checkClipboard();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  Future<void> _checkClipboard() async {
    final has = await _clipboard.hasImage();
    if (mounted && has != _clipboardImage) setState(() => _clipboardImage = has);
  }

  /// Paste the clipboard's image (⌘⇧V on a hardware keyboard).
  Future<void> _pasteImage() async {
    final image = await _clipboard.read();
    if (!mounted) return;
    if (image == null) {
      toast(context, 'There is no image on the clipboard.');
      return;
    }
    await _addImage(image.$1, image.$2);
  }

  /// The edit menu, with Paste Image when the clipboard holds an image
  /// (Flutter's own Paste only handles text).
  Widget _menu(BuildContext context, EditableTextState field) {
    final items = [...field.contextMenuButtonItems];
    if (_clipboardImage) {
      final at = items.indexWhere((i) => i.type == ContextMenuButtonType.paste);
      final paste = ContextMenuButtonItem(
        label: at < 0 ? 'Paste' : 'Paste Image',
        onPressed: () {
          ContextMenuController.removeAny();
          _pasteImage();
        },
      );
      items.insert(at < 0 ? 0 : at + 1, paste);
    }
    return AdaptiveTextSelectionToolbar.buttonItems(anchors: field.contextMenuAnchors, buttonItems: items);
  }

  /// Sends the message. With [now] it interrupts a running answer; without, it
  /// waits behind it (queued).
  void _send({bool now = false}) {
    final text = _text.text;
    final bloc = context.read<ChatBloc>();
    if (text.trim().isEmpty && bloc.state.images.isEmpty && bloc.state.files.isEmpty) return;
    bloc.add(ChatMessageSent(text, now: now));
    _text.clear();
  }

  /// An image pasted from the clipboard or inserted by the keyboard (Gboard):
  /// kept as a file, then uploaded like a photo.
  Future<void> _insert(KeyboardInsertedContent content) async {
    final data = content.data;
    if (data == null || data.isEmpty) {
      toast(context, 'This image could not be read; pick it from the photo library.');
      return;
    }
    final extension = switch (content.mimeType) {
      'image/png' => 'png',
      'image/gif' => 'gif',
      'image/webp' => 'webp',
      'image/bmp' => 'bmp',
      _ => 'jpg',
    };
    await _addImage(data, extension);
  }

  /// Keep pasted bytes as a file; it is then uploaded like a photo.
  Future<void> _addImage(Uint8List data, String extension) async {
    final bloc = context.read<ChatBloc>();
    final dir = await Directory.systemTemp.createTemp('aurelius-paste');
    final file = File('${dir.path}/Pasted image.$extension');
    await file.writeAsBytes(data);
    bloc.add(ChatImageAdded(file, 'Pasted image.$extension'));
  }

  /// While an answer runs, sending interrupts it ("Send now"); long press or ⌥↩ queues instead.
  void _sendDefault() => _send(now: context.read<ChatBloc>().state.streaming != null);

  void _queue() => _send();

  /// "+": a photo or file, and what this agent may use (the browser, the
  /// screen), each on or off.
  Future<void> _attach(ChatState state) async {
    final bloc = context.read<ChatBloc>();
    Widget tool(BuildContext sheet, String value, IconData icon, String title, String subtitle, bool on) => ListTile(
          key: Key('composer-tool-$value'),
          leading: Icon(icon, color: on ? Tokens.warning : null),
          title: Text(title),
          subtitle: Text(subtitle, style: TextStyle(fontSize: 12.5, color: Tokens.muted)),
          trailing: Switch.adaptive(value: on, onChanged: (_) => Navigator.pop(sheet, value)),
          onTap: () => Navigator.pop(sheet, value),
        );
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(enabled: !state.uploading, leading: const Icon(Icons.photo_camera_outlined), title: const Text('Take a photo'), onTap: () => Navigator.pop(sheet, 'camera')),
            ListTile(enabled: !state.uploading, leading: const Icon(Icons.photo_library_outlined), title: const Text('Photo library'), onTap: () => Navigator.pop(sheet, 'gallery')),
            ListTile(enabled: !state.uploading, leading: const Icon(Icons.description_outlined), title: const Text('File'), onTap: () => Navigator.pop(sheet, 'file')),
            const Divider(height: 1),
            tool(sheet, 'browser', Icons.language, 'Browser', 'Let this agent use the browser.', state.browserAllowed),
            tool(sheet, 'screen', Icons.screenshot_monitor, 'Screen', 'Screenshots of the PC without asking.', state.screenAllowed),
          ],
        ),
      ),
    );
    if (choice == null || !mounted) return;
    if (choice == 'browser') return _toggleBrowser(bloc.state);
    if (choice == 'screen') return _toggleScreen(bloc.state);
    if (choice == 'file') {
      final picked = [for (final f in await FilePicker.pickFiles()) if (f.path != null) (File(f.path!), f.name)];
      if (picked.isNotEmpty) bloc.add(ChatPicked(picked));
    } else if (choice == 'gallery') {
      final room = ChatBloc.maxImages - bloc.state.images.length;
      if (room < 1) {
        if (mounted) toast(context, 'At most ${ChatBloc.maxImages} images per message.');
        return;
      }
      final images = await ImagePicker().pickMultiImage(imageQuality: 85, maxWidth: 2400, limit: room);
      if (images.isNotEmpty) bloc.add(ChatPicked([for (final i in images) (File(i.path), i.name)], images: true));
    } else {
      final image = await ImagePicker().pickImage(source: ImageSource.camera, imageQuality: 85, maxWidth: 2400);
      if (image != null) bloc.add(ChatImageAdded(File(image.path), image.name));
    }
  }

  /// Turning the screen on asks first: screenshots show anything that is open.
  /// Off, the agent still asks for each screenshot.
  Future<void> _toggleScreen(ChatState state) async {
    final bloc = context.read<ChatBloc>();
    if (state.screenAllowed) {
      bloc.add(const ChatScreenToggled());
      return;
    }
    final ok = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Let this agent see your screen?'),
        content: const Text(
          'It can then take screenshots of the PC screen or of one window without asking. '
          'A screenshot shows anything that is open. Off, it still asks you for each screenshot.',
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(dialog, true), child: const Text('Allow')),
        ],
      ),
    );
    if (ok == true && mounted) bloc.add(const ChatScreenToggled());
  }

  /// Turning the browser on asks first: it is the user's own Chrome, with their logins.
  Future<void> _toggleBrowser(ChatState state) async {
    final bloc = context.read<ChatBloc>();
    if (state.browserAllowed) {
      bloc.add(const ChatBrowserToggled());
      return;
    }
    final ok = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Allow the browser?'),
        content: const Text(
          'This agent can then use your own Chrome, which is signed in to your accounts. '
          'Every browser action is still shown to you first, and the browser stays off for task workers.',
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(dialog, true), child: const Text('Allow')),
        ],
      ),
    );
    if (ok == true && mounted) bloc.add(const ChatBrowserToggled());
  }

  Future<void> _pickMode(AgentMode current) async {
    final bloc = context.read<ChatBloc>();
    final mode = await showModalBottomSheet<AgentMode>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final m in agentModes)
              ListTile(
                selected: m == current,
                leading: Icon(m == current ? Icons.radio_button_checked : Icons.radio_button_unchecked, color: m == AgentMode.auto ? Tokens.warning : null),
                title: Text(m.label),
                trailing: InfoIcon(
                  switch (m) {
                    AgentMode.ask => 'Ask before every change outside the chat.',
                    AgentMode.acceptEdits => 'Change files in the project folder without asking.',
                    AgentMode.auto => 'Do everything that would ask. Deny rules still apply.',
                    AgentMode.plan => 'Only read and plan; change nothing.',
                  },
                  label: m.label,
                ),
                onTap: () => Navigator.pop(sheet, m),
              ),
          ],
        ),
      ),
    );
    if (mode != null && mode != current) bloc.add(ChatModeSelected(mode));
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<ChatBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final readOnly = state.history || (link.isOnline && !link.can('chat'));
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): _sendDefault,
        const SingleActivator(LogicalKeyboardKey.enter, control: true): _sendDefault,
        const SingleActivator(LogicalKeyboardKey.enter, alt: true): _queue,
        const SingleActivator(LogicalKeyboardKey.keyV, meta: true, shift: true): _pasteImage,
        const SingleActivator(LogicalKeyboardKey.keyV, control: true, shift: true): _pasteImage,
        const SingleActivator(LogicalKeyboardKey.tab, shift: true): () => context.read<ChatBloc>().add(ChatModeSelected(state.mode.next)),
      },
      child: Container(
        decoration: BoxDecoration(color: Tokens.plain, border: Border(top: BorderSide(color: Tokens.sep, width: 0.5))),
        padding: EdgeInsets.fromLTRB(12, 8, 12, 8 + MediaQuery.paddingOf(context).bottom),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (state.images.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: SizedBox(
                  height: 60,
                  child: ListView(
                    scrollDirection: Axis.horizontal,
                    children: [
                      for (final i in state.images)
                        Padding(
                          padding: const EdgeInsets.only(right: 6, top: 4),
                          child: Stack(
                            clipBehavior: Clip.none,
                            children: [
                              _Thumbnail(file: i.local),
                              Positioned(
                                top: -6,
                                right: -6,
                                child: GestureDetector(
                                  key: Key('remove-image-${i.name}'),
                                  onTap: () => context.read<ChatBloc>().add(ChatImageRemoved(i)),
                                  child: Container(
                                    decoration: BoxDecoration(color: Tokens.surface2, shape: BoxShape.circle, border: Border.all(color: Tokens.border)),
                                    padding: const EdgeInsets.all(2),
                                    child: Icon(Icons.close, size: 14, color: Tokens.text),
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            if (state.files.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: SizedBox(
                  height: 40,
                  child: ListView(
                    scrollDirection: Axis.horizontal,
                    children: [
                      for (final f in state.files)
                        Padding(
                          padding: const EdgeInsets.only(right: 6),
                          child: InputChip(
                            key: Key('pending-file-${f.name}'),
                            avatar: const Icon(Icons.description_outlined, size: 16),
                            label: Text('${f.name} · ${Fmt.bytes(f.byteLength)}'),
                            onDeleted: () => context.read<ChatBloc>().add(ChatFileRemoved(f)),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            if (state.attachments.isNotEmpty || state.uploading)
              SizedBox(
                height: 40,
                child: ListView(
                  scrollDirection: Axis.horizontal,
                  children: [
                    for (final a in state.attachments)
                      Padding(
                        padding: const EdgeInsets.only(right: 6),
                        child: InputChip(
                          avatar: const Icon(Icons.attach_file, size: 16),
                          label: Text('${a.name} · ${Fmt.bytes(a.byteLength)}'),
                          onDeleted: () => context.read<ChatBloc>().add(ChatAttachmentRemoved(a.id)),
                        ),
                      ),
                    if (state.uploading) const Center(child: Padding(padding: EdgeInsets.all(8), child: SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)))),
                  ],
                ),
              ),
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                IconButton(
                  key: const Key('composer-plus'),
                  tooltip: 'Attach, web, browser, screen',
                  onPressed: readOnly ? null : () => _attach(state),
                  icon: const Icon(CupertinoIcons.plus_circle),
                  style: IconButton.styleFrom(fixedSize: const Size.square(42), minimumSize: const Size.square(42)),
                ),
                const SizedBox(width: 4),
                Expanded(
                  child: TextField(
                    controller: _text,
                    focusNode: _focus,
                    enabled: !readOnly,
                    minLines: 1,
                    maxLines: 6,
                    textCapitalization: TextCapitalization.sentences,
                    onTap: _checkClipboard,
                    contextMenuBuilder: _menu,
                    contentInsertionConfiguration: ContentInsertionConfiguration(
                      allowedMimeTypes: const ['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/bmp'],
                      onContentInserted: _insert,
                    ),
                    decoration: InputDecoration(
                      hintText: state.history ? 'Read-only: an earlier chat' : readOnly ? 'Read-only: this phone may not chat' : 'Message',
                      isDense: true,
                      fillColor: Tokens.cell2,
                      contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 11),
                      border: OutlineInputBorder(borderRadius: BorderRadius.circular(21), borderSide: BorderSide.none),
                      enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(21), borderSide: BorderSide.none),
                      disabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(21), borderSide: BorderSide.none),
                      focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(21), borderSide: BorderSide.none),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                // One button: Stop while an answer runs and nothing is typed; else Send, which
                // interrupts the running answer ("Send now"). Long press queues the message instead.
                ValueListenableBuilder<TextEditingValue>(
                  valueListenable: _text,
                  builder: (context, value, _) {
                    final running = state.streaming;
                    final empty = value.text.trim().isEmpty && state.images.isEmpty && state.files.isEmpty;
                    final style = IconButton.styleFrom(backgroundColor: Tokens.accent, foregroundColor: Tokens.accentInk, fixedSize: const Size.square(42), minimumSize: const Size.square(42));
                    final runId = running?.runId;
                    if (runId != null && empty) {
                      return IconButton.filled(
                        key: const Key('composer-stop'),
                        tooltip: 'Stop the answer',
                        onPressed: readOnly ? null : () => context.read<ChatBloc>().add(ChatStopRequested(runId)),
                        icon: const Icon(Icons.stop_rounded, size: 22),
                        style: style,
                      );
                    }
                    // The tooltip must not claim the long press (it does on touch screens), so it shows on hover only.
                    return Tooltip(
                      message: running != null ? 'Send now: interrupt the answer (⌘↩). Hold to send after it (⌥↩)' : 'Send (⌘↩)',
                      triggerMode: TooltipTriggerMode.manual,
                      child: GestureDetector(
                        onLongPress: readOnly || running == null || empty ? null : _queue,
                        child: IconButton.filled(
                          key: const Key('composer-send'),
                          onPressed: readOnly ? null : _sendDefault,
                          icon: const Icon(CupertinoIcons.arrow_up, size: 20),
                          style: style,
                        ),
                      ),
                    );
                  },
                ),
              ],
            ),
            const SizedBox(height: 6),
            // Mode and model, then what is switched on under "+" (tap to switch it off).
            SingleChildScrollView(
              key: const Key('composer-chips'),
              scrollDirection: Axis.horizontal,
              child: Row(
                children: [
                  ActionChip(
                    avatar: Icon(Icons.tune, size: 16, color: state.mode == AgentMode.auto ? Tokens.warning : Tokens.text2),
                    label: Text(state.mode.label, style: TextStyle(color: state.mode == AgentMode.auto ? Tokens.warning : Tokens.text2)),
                    onPressed: readOnly ? null : () => _pickMode(state.mode),
                  ),
                  const SizedBox(width: 6),
                  const ConversationModelChip(),
                  if (state.browserAllowed) ...[
                    const SizedBox(width: 6),
                    InputChip(
                      key: const Key('browser-chip'),
                      avatar: Icon(Icons.language, size: 16, color: Tokens.warning),
                      label: const Text('Browser'),
                      onDeleted: readOnly ? null : () => _toggleBrowser(state),
                    ),
                  ],
                  if (state.screenAllowed) ...[
                    const SizedBox(width: 6),
                    InputChip(
                      key: const Key('screen-chip'),
                      avatar: Icon(Icons.screenshot_monitor, size: 16, color: Tokens.warning),
                      label: const Text('Screen'),
                      onDeleted: readOnly ? null : () => _toggleScreen(state),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A file sent with a message to an employee: name and size.
/// A small action in a glass capsule under a message (Send now, Delete).
class _GlassPill extends StatelessWidget {
  const _GlassPill({super.key, required this.label, required this.onTap, this.accent = false, this.danger = false});

  final String label;
  final VoidCallback onTap;
  final bool accent;
  final bool danger;

  @override
  Widget build(BuildContext context) => Glass(
        flat: true,
        radius: 14,
        child: Material(
          type: MaterialType.transparency,
          child: InkWell(
            onTap: onTap,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 5),
              child: Text(label, style: TextStyle(fontSize: 13, fontWeight: FontWeight.w500, color: danger ? Tokens.negative : accent ? Tokens.accentText : Tokens.label)),
            ),
          ),
        ),
      );
}

class _UploadChip extends StatelessWidget {
  const _UploadChip({required this.name, required this.byteLength, this.path, this.file});

  final String name;
  final int byteLength;

  /// In the work folder; a Markdown file opens in the viewer on tap.
  final String? path;

  /// The sent file; other files open from the server on tap (when it can send them).
  final MessageFileView? file;

  @override
  Widget build(BuildContext context) {
    final target = path;
    final scope = chatFileScope(context);
    final sent = file;
    final fetchable = sent != null && uploadFetchable(context, sent);
    final icon = Icon(Icons.description_outlined, size: 14, color: Tokens.text2);
    final chip = Container(
      constraints: const BoxConstraints(maxWidth: 240),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(color: Tokens.surface, border: Border.all(color: Tokens.border), borderRadius: BorderRadius.circular(8)),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (fetchable && isImageName(name)) UploadThumb(id: sent.id!, width: 28, height: 28, fallback: icon) else icon,
          const SizedBox(width: 6),
          Flexible(child: Text(name, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12.5, color: Tokens.text))),
          const SizedBox(width: 6),
          Text(Fmt.bytes(byteLength), style: TextStyle(fontSize: 12, color: Tokens.muted)),
        ],
      ),
    );
    if (target != null && scope != null && isMarkdownPath(target)) {
      return GestureDetector(key: Key('upload-md-$target'), onTap: () => openMarkdown(context, scope, target), child: chip);
    }
    if (sent == null || !fetchable) return chip;
    return GestureDetector(key: Key('upload-${sent.id}'), onTap: () => openUpload(context, sent), child: chip);
  }
}

/// A small square preview of an image on this phone.
class _Thumbnail extends StatelessWidget {
  const _Thumbnail({required this.file});

  final File file;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(8),
      child: Image.file(
        file,
        width: 52,
        height: 52,
        fit: BoxFit.cover,
        cacheWidth: 156,
        errorBuilder: (_, _, _) => Container(width: 52, height: 52, color: Tokens.surface2, child: Icon(Icons.image_outlined, color: Tokens.muted)),
      ),
    );
  }
}

/// What a conversation with an employee (or a project's assistant) runs on:
/// the default without a choice here (employee → project → global), the task's
/// own choice in a task conversation, and this conversation's own choice.
class ConversationRun {
  const ConversationRun({required this.fallback, required this.task, required this.model, required this.effort});

  /// The employee's (or project's) default.
  final EmployeeRunView fallback;

  /// The task behind a task conversation; its own model and effort come below
  /// this conversation's.
  final TaskView? task;

  /// Chosen for this conversation (`conversation.set_run`).
  final String? model;
  final String? effort;

  static ConversationRun? of(BuildContext context, {required RuntimeState runtime, required ChatState chat, CompanyState? company}) {
    final snapshot = runtime.snapshot;
    final bloc = context.read<ChatBloc>();
    if (snapshot == null) return null;
    final id = chat.conversationId;
    final task = id == null || company == null ? null : company.tasks.where((t) => t.conversationId == id).firstOrNull;
    final employeeId = bloc.employeeId;
    final fallback = employeeId != null
        ? snapshot.employeeRun(employeeId, projectId: bloc.projectId ?? task?.projectId)
        : snapshot.projectRun(bloc.projectId!);
    return ConversationRun(fallback: fallback, task: task, model: chat.runModel, effort: chat.runEffort);
  }

  bool get effortShown => fallback.connection?.supportsEffort ?? true;
  String? get defaultModel => fallback.model.value;
  String? get defaultEffort => fallback.effort.value;

  /// What the next answer runs on.
  String? get effectiveModel => model ?? task?.run.model ?? defaultModel;
  String? get effectiveEffort => effort ?? task?.run.effort ?? defaultEffort;

  /// Runs on something other than the default (a choice equal to it is none).
  bool get modelOverridden => effectiveModel != defaultModel;
  bool get effortOverridden => effortShown && effectiveEffort != defaultEffort;
  bool get overridden => modelOverridden || effortOverridden;

  /// The task behind this conversation chose its own model or effort.
  bool get taskChose => task?.run.model != null || task?.run.effort != null;

  /// This conversation's choice that brings the model (or effort) back to the
  /// default, as the desktop: without a task choice none, so the conversation
  /// follows the default again. Over a task's own choice the default's resolved
  /// value (employee → project → global), sent explicitly so the task's choice
  /// below it cannot apply. Null there only when nothing is set anywhere (the
  /// runtime decides); then a task's own effort still applies until the server
  /// can inherit (v0.9.1).
  String? get modelReset => taskChose ? defaultModel : null;
  String? get effortReset => taskChose ? defaultEffort : null;

  /// Back to the default; says so when a task's own effort keeps applying.
  void reset(BuildContext context, ChatBloc chat) {
    chat.add(ChatRunSelected(model: modelReset, effort: effortReset));
    if (effortShown && effortReset == null && task?.run.effort != null) {
      toast(context, 'The default sets no effort, so the task\'s own effort (${ModelCatalog.effortLabel(task!.run.effort)}) still applies.');
    }
  }
}

/// "Opus 5.5 · High": a resolved model and effort.
String runLabel(String? model, String? effort, {bool effortShown = true}) => [
      model == null ? 'Default' : ModelCatalog.friendly(model),
      if (effortShown && effort != null) ModelCatalog.effortLabel(effort),
    ].join(' · ');

ConversationRun? _watchRun(BuildContext context) {
  final runtime = context.watch<RuntimeBloc>().state;
  final chat = context.watch<ChatBloc>().state;
  // Project conversations have no employee and no task.
  final company = context.read<ChatBloc>().employeeId == null ? null : context.watch<CompanyBloc>().state;
  return ConversationRun.of(context, runtime: runtime, chat: chat, company: company);
}

class ConversationModelChip extends StatelessWidget {
  const ConversationModelChip({super.key});

  @override
  Widget build(BuildContext context) {
    final run = _watchRun(context);
    final history = context.select<ChatBloc, bool>((b) => b.state.history);
    final overridden = run?.overridden ?? false;
    final label = run == null || run.effectiveModel == null && !overridden
        ? 'Model'
        : [runLabel(run.effectiveModel, run.effectiveEffort, effortShown: run.effortShown), if (overridden) 'override'].join(' · ');
    final avatar = Icon(Icons.auto_awesome_outlined, size: 16, color: overridden ? Tokens.accentText : Tokens.text2);
    final text = Text(label, overflow: TextOverflow.ellipsis, style: TextStyle(color: overridden ? Tokens.accentText : Tokens.text2));
    final open = history ? null : () => _showConversationModelSheet(context);
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 240),
      // An override resets in one tap on the chip's undo, as on the desktop.
      child: overridden && !history
          ? InputChip(
              key: const Key('conversation-model-chip'),
              avatar: avatar,
              label: text,
              onPressed: open,
              deleteIcon: Icon(Icons.undo, size: 16, color: Tokens.accentText),
              deleteButtonTooltipMessage: context.read<ChatBloc>().employeeId == null ? 'Back to project default' : 'Back to employee default',
              onDeleted: () => run!.reset(context, context.read<ChatBloc>()),
            )
          : ActionChip(key: const Key('conversation-model-chip'), avatar: avatar, label: text, onPressed: open),
    );
  }
}

/// Model and effort for this chat (or task) alone: the employee's default
/// first, with what it resolves to, then the connection's models and the
/// effort levels. Nothing here changes the employee's or the global default;
/// those are set on the employee page.
void _showConversationModelSheet(BuildContext context) {
  final runtime = context.read<RuntimeBloc>();
  final chat = context.read<ChatBloc>();
  final employeeId = chat.employeeId;
  final company = employeeId == null ? null : context.read<CompanyBloc>();
  final name = employeeId == null ? null : company?.state.employee(employeeId)?.name;
  final whose = employeeId == null ? 'Project default' : 'Employee default';
  if (runtime.state.snapshot == null) {
    toast(context, runtime.state.loading ? 'Loading…' : 'Not available while offline.');
    return;
  }
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => MultiBlocProvider(
      providers: [BlocProvider.value(value: runtime), BlocProvider.value(value: chat), if (company != null) BlocProvider.value(value: company)],
      child: Builder(
        builder: (context) {
          final run = _watchRun(context);
          if (run == null) return const SizedBox.shrink();
          final scope = run.task == null ? 'chat' : 'task';
          final connection = run.fallback.connection;
          void choose({required String? model, required String? effort}) => chat.add(ChatRunSelected(model: model, effort: effort));

          Widget option(bool selected, String title, {String? detail, Key? key, required VoidCallback onTap}) => ListTile(
                key: key,
                leading: Icon(selected ? Icons.radio_button_checked : Icons.radio_button_unchecked, color: selected ? Tokens.accentText : Tokens.muted),
                title: Text(title),
                subtitle: detail == null ? null : Text(detail, style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                onTap: onTap,
              );

          return SafeArea(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.8),
              child: ListView(
                shrinkWrap: true,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
                    child: Text('This $scope', style: Tokens.title(size: 20)),
                  ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                    child: Text(
                      [
                        if (connection != null) connection.displayName,
                        name == null ? 'only this $scope; the default stays as it is' : 'only this $scope; $name\'s default stays as it is',
                      ].join(' · '),
                      style: TextStyle(color: Tokens.muted, fontSize: 12.5),
                    ),
                  ),
                  option(
                    !run.overridden,
                    '$whose (${runLabel(run.defaultModel, run.defaultEffort, effortShown: run.effortShown)})',
                    detail: name == null ? null : 'Changed on $name\'s page',
                    key: const Key('conversation-run-default'),
                    onTap: () => run.reset(context, chat),
                  ),
                  if (run.overridden)
                    ListTile(
                      key: const Key('conversation-run-reset'),
                      leading: Icon(Icons.undo, color: Tokens.accentText),
                      title: Text('Back to ${whose.toLowerCase()}', style: TextStyle(color: Tokens.accentText)),
                      onTap: () {
                        run.reset(context, chat);
                        Navigator.pop(sheet);
                      },
                    ),
                  const Eyebrow('Model'),
                  if (connection == null || connection.models.isEmpty)
                    ListTile(title: Text('This connection reports no models.', style: TextStyle(color: Tokens.muted))),
                  for (final m in connection?.models ?? const <String>[])
                    option(run.modelOverridden && run.effectiveModel == m, ModelCatalog.friendly(m),
                        detail: ModelCatalog.describe(m), onTap: () => choose(model: m, effort: run.effort)),
                  if (run.effortShown) ...[
                    const Eyebrow('Effort'),
                    option(!run.effortOverridden, '$whose (${ModelCatalog.effortLabel(run.defaultEffort)})',
                        onTap: () => choose(model: run.model, effort: run.effortReset)),
                    for (final (value, label, detail) in ModelCatalog.efforts)
                      option(run.effortOverridden && run.effectiveEffort == value, label, detail: detail, onTap: () => choose(model: run.model, effort: value)),
                  ],
                ],
              ),
            ),
          );
        },
      ),
    ),
  );
}

/// Models of the active connection, then effort where the runtime supports it.
void showModelSheet(BuildContext context, Purpose purpose) {
  final bloc = context.read<RuntimeBloc>();
  final choice = bloc.state.choice(purpose);
  if (choice == null) {
    toast(context, 'Choose a connection in Aurelius on the PC first.');
    return;
  }
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => BlocProvider.value(
      value: bloc,
      child: BlocBuilder<RuntimeBloc, RuntimeState>(
        builder: (context, state) {
          final current = state.choice(purpose) ?? choice;
          final connection = current.connection;
          return SafeArea(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.8),
              child: ListView(
                shrinkWrap: true,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
                    child: Text(purpose == Purpose.employee ? 'Model for employees' : 'Model', style: Tokens.title(size: 20)),
                  ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                    child: Text('${connection.displayName} · applies to new answers', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                  ),
                  if (connection.models.isEmpty)
                    ListTile(title: Text('This connection reports no models.', style: TextStyle(color: Tokens.muted))),
                  for (final m in connection.models)
                    ListTile(
                      leading: Icon(m == current.model ? Icons.radio_button_checked : Icons.radio_button_unchecked,
                          color: m == current.model ? Tokens.accentText : Tokens.muted),
                      title: Text(ModelCatalog.friendly(m)),
                      subtitle: ModelCatalog.describe(m) == null ? null : Text(ModelCatalog.describe(m)!, style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                      onTap: () => bloc.add(RuntimeModelSelected(purpose, m)),
                    ),
                  if (connection.supportsEffort) ...[
                    const Eyebrow('Effort'),
                    ListTile(
                      leading: Icon(current.effort == null ? Icons.radio_button_checked : Icons.radio_button_unchecked,
                          color: current.effort == null ? Tokens.accentText : Tokens.muted),
                      title: const Text('Default'),
                      subtitle: Text('Claude Code decides', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                      onTap: () => bloc.add(RuntimeEffortSelected(purpose, null)),
                    ),
                    for (final (value, label, detail) in ModelCatalog.efforts)
                      ListTile(
                        leading: Icon(current.effort == value ? Icons.radio_button_checked : Icons.radio_button_unchecked,
                            color: current.effort == value ? Tokens.accentText : Tokens.muted),
                        title: Text(label),
                        subtitle: Text(detail, style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                        onTap: () => bloc.add(RuntimeEffortSelected(purpose, value)),
                      ),
                  ],
                  if (purpose == Purpose.employee) ...[
                    const Eyebrow('Subagents'),
                    ...runRows(context, state, RunTarget.global(Purpose.subagent)),
                  ],
                  Padding(
                    padding: EdgeInsets.fromLTRB(16, 8, 16, 12),
                    child: Text('Connections and API keys are managed in Aurelius on the PC.', style: TextStyle(color: Tokens.muted, fontSize: 12)),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    ),
  );
}
