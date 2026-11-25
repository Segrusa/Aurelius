import 'dart:async';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:go_router/go_router.dart';
import 'package:image_picker/image_picker.dart';

import '../../core/clipboard/image_clipboard.dart';
import '../../core/net/compacting.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart';
import '../../data/discussion.dart';
import '../../data/everyone.dart';
import '../../data/links.dart' show messageLocateFeature;
import '../../data/views.dart';
import '../../data/markdown_paths.dart';
import '../../data/run_config.dart';
import '../../data/run_decisions.dart';
import '../runtime/runtime_bloc.dart';
import '../company/company_bloc.dart';
import 'everyone_audience.dart';
import '../connection/connection_bloc.dart';
import '../company_org/employee_run_sheet.dart';
import '../inbox/inbox_bloc.dart';
import '../load/load_widgets.dart';
import 'mentions.dart';
import 'answer_body.dart';
import 'chat_bloc.dart';
import 'collapse.dart';
import 'decision_card.dart';
import 'secret_check.dart';
import 'file_page.dart';
import '../links/link_actions.dart' show copyMessageLink;
import '../markdown/links.dart';
import '../markdown/markdown_page.dart';
import '../markdown/mention_markdown.dart';
import 'following_list.dart';
import 'message_edit.dart';
import 'live_steps.dart';
import 'subagent_chip.dart';
import 'upload_files.dart';
import '../sessions/session_screen_rules.dart' show answerFooter;

MarkdownStyleSheet markdownStyle(BuildContext context, {double size = 15}) {
  final base = MarkdownStyleSheet.fromTheme(Theme.of(context));
  return base.copyWith(
    p: TextStyle(fontSize: size, height: 23 / 15, color: Tokens.text),
    h1: Tokens.title(size: 24),
    h2: Tokens.title(size: 20),
    h3: TextStyle(fontSize: 16, fontWeight: FontWeight.w600, color: Tokens.text),
    code: TextStyle(fontFamily: Tokens.mono, fontSize: size - 2, color: Tokens.accentDeep, backgroundColor: Tokens.surface2),
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

/// The line under a queued message: why its answer has not started. A server that does not
/// say (`queuedFor` left out) means it waits for the answer before it. Same words as the desktop.
String queuedLine(String? queuedFor) =>
    queuedFor == 'machine' ? 'Waiting · the machine is busy, starts when there is room' : 'Waiting · sends after the current answer';

/// Whether a long press on [message] offers Copy link (§6.1): the server can
/// find it again (`message_locate`, only in a channel or a ticket's discussion),
/// and it is a finished message of someone, not a queued or streaming one.
bool messageLinkable(BuildContext context, MessageView? message) {
  if (message == null || message.isStreaming || message.queued || message.system != null) return false;
  final bloc = context.read<ChatBloc>();
  if (!bloc.group && bloc.ticketId == null) return false;
  return context.read<AureliusRepository>().linkState.has(messageLocateFeature);
}

/// Messages and composer of one conversation (chat or employee).
class ChatView extends StatefulWidget {
  const ChatView({
    super.key,
    this.emptyTitle = 'What can I do for you?',
    this.emptyDetail,
    this.composer = true,
    this.readOnlyNote,
    this.hint,
    this.source,
    this.taskProjectId,
    this.composerText,
    this.composerActions,
    this.aboveComposer,
    this.draftSlot,
    this.readOnlyAction,
    this.revealMessage,
    this.floating = false,
    this.topInset = 0,
    this.modeFixed = false,
  });

  final String emptyTitle;
  final String? emptyDetail;

  /// `false` for a conversation that cannot be written to.
  final bool composer;

  /// Without a composer: why it cannot be written to (a ticket on Done, an archived project's channel).
  final String? readOnlyNote;

  /// The composer's hint, in place of the usual one ("Message Forge (owner) · …").
  final String? hint;

  /// Where this chat is: its messages can become a ticket (MU2-10). Null: they cannot.
  final TicketSource? source;

  /// The project a system line's task is in (a ticket's discussion), to open it.
  final String? taskProjectId;

  /// The composer's text, when the page acts on it too (Send back, Pas aan).
  final TextEditingController? composerText;

  /// Where the composer's unsent text is kept (R3 M1); null: the chat's own
  /// (its conversation, else its employee, project or channel).
  final String? draftSlot;

  /// Under the composer: the decisions of a ticket's discussion (Plan review, Review).
  final Widget Function(BuildContext context, TextEditingController text)? composerActions;

  /// Right above the composer: the decision of the moment in a ticket's session (its gate).
  final Widget? aboveComposer;

  /// After [readOnlyNote]: a link, such as "Open ticket".
  final Widget? readOnlyAction;

  /// A link opened here: scroll to this message once it is read, and mark it.
  final String? revealMessage;

  /// The session screen (design "Aurelius sessiescherm"): the messages run
  /// under a floating head [topInset] high and under a floating composer.
  final bool floating;
  final double topInset;

  /// The mode cannot change here: a design session always works in Auto (the server refuses
  /// another mode), so the composer shows no mode to pick.
  final bool modeFixed;

  @override
  State<ChatView> createState() => _ChatViewState();
}

class _ChatViewState extends State<ChatView> {
  /// What is typed in the composer; quick replies give way to it.
  final _draft = ValueNotifier<String>('');

  /// The floating composer's height: the messages end above it.
  final _bottom = ValueNotifier<double>(0);

  @override
  void dispose() {
    _draft.dispose();
    _bottom.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final emptyTitle = widget.emptyTitle;
    final emptyDetail = widget.emptyDetail;
    // A write refused with `channel_archived`: the read-only bar, never the error (AUR-CHAT-001 §8).
    final closed = context.select<ChatBloc, bool>((b) => b.state.closed);
    final composer = widget.composer && !closed;
    final readOnlyNote = widget.readOnlyNote;
    final bloc = context.read<ChatBloc>();
    return BlocListener<ChatBloc, ChatState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<ChatBloc>().add(const ChatNoticeShown());
      },
      child: BlocListener<ChatBloc, ChatState>(
        listenWhen: (a, b) => b.secretCheck != null && a.secretCheck != b.secretCheck,
        listener: (context, state) {
          final bloc = context.read<ChatBloc>()..add(const ChatSecretCheckShown());
          final unsent = state.unsent.where((u) => u.localId == state.secretCheck).firstOrNull;
          if (unsent != null) unawaited(sendAnywayAsked(context, bloc, unsent));
        },
        child: BlocListener<RuntimeBloc, RuntimeState>(
        listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
        listener: (context, state) {
          toast(context, state.notice!);
          context.read<RuntimeBloc>().add(const RuntimeNoticeShown());
        },
        child: _layout(
          messages: (padding) => _Messages(emptyTitle: emptyTitle, emptyDetail: emptyDetail, config: widget, padding: padding),
          chrome: [
            BlocSelector<ChatBloc, ChatState, bool>(
              selector: (s) => s.awaitingStart,
              builder: (context, waiting) => LoadBanner(waiting: waiting),
            ),
            // The group chat has no work folder and no subagents of its own.
            if (bloc.state.conversationId != null && !bloc.group)
              Padding(
                padding: const EdgeInsets.only(top: 6, bottom: 10),
                child: Row(
                children: [
                  const Spacer(),
                  BlocSelector<ChatBloc, ChatState, bool>(
                    selector: (s) => s.streaming != null,
                    builder: (context, running) => SubagentChip(key: ValueKey('subagents-${bloc.state.conversationId}'), conversationId: bloc.state.conversationId!, running: running, employee: true),
                  ),
                ],
              ),
              ),
            ?widget.aboveComposer,
            if (composer) _QuickRepliesCard(draft: _draft),
            if (composer) _Composer(draft: _draft, config: widget),
            if (!composer && readOnlyNote != null)
              Container(
                key: const Key('chat-read-only'),
                width: double.infinity,
                decoration: widget.floating
                    ? BoxDecoration(color: Tokens.cell, border: Border.all(color: Tokens.sep), borderRadius: BorderRadius.circular(Tokens.cardRadius))
                    : BoxDecoration(color: Tokens.plain, border: Border(top: BorderSide(color: Tokens.sep, width: 0.5))),
                margin: widget.floating ? EdgeInsets.fromLTRB(16, 0, 16, 12 + MediaQuery.paddingOf(context).bottom) : null,
                padding: widget.floating ? const EdgeInsets.fromLTRB(16, 12, 16, 12) : EdgeInsets.fromLTRB(20, 12, 20, 12 + MediaQuery.paddingOf(context).bottom),
                child: Wrap(
                  alignment: WrapAlignment.center,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: 8,
                  children: [
                    Text(readOnlyNote, textAlign: TextAlign.center, style: TextStyle(color: Tokens.label2, fontSize: 13.5)),
                    ?widget.readOnlyAction,
                  ],
                ),
              ),
          ],
        ),
      ),
      ),
    );
  }

  /// The messages above the composer and the rest under them; on the session
  /// screen the messages fill the page and the rest floats over its bottom,
  /// on a fade of the page colour.
  Widget _layout({required Widget Function(EdgeInsets padding) messages, required List<Widget> chrome}) {
    if (!widget.floating) return Column(children: [Expanded(child: messages(FollowingList.defaultPadding)), ...chrome]);
    final bg = Tokens.bg;
    return Stack(children: [
      Positioned.fill(
        child: ValueListenableBuilder<double>(
          valueListenable: _bottom,
          builder: (context, bottom, _) => messages(EdgeInsets.fromLTRB(20, widget.topInset + 8, 20, bottom + 8)),
        ),
      ),
      Positioned(
        left: 0,
        right: 0,
        bottom: 0,
        child: MeasuredSize(
          onSize: (size) => _bottom.value = size.height,
          child: DecoratedBox(
            decoration: BoxDecoration(gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [bg.withValues(alpha: 0), bg], stops: const [0, 0.32])),
            child: Padding(padding: const EdgeInsets.only(top: 24), child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: chrome)),
          ),
        ),
      ),
    ]);
  }
}

/// Tells its child's size after each layout that changed it.
class MeasuredSize extends SingleChildRenderObjectWidget {
  const MeasuredSize({super.key, required this.onSize, required super.child});

  final ValueChanged<Size> onSize;

  @override
  RenderObject createRenderObject(BuildContext context) => RenderMeasuredSize(onSize);

  @override
  void updateRenderObject(BuildContext context, RenderMeasuredSize renderObject) => renderObject.onSize = onSize;
}

/// The render object of [MeasuredSize].
class RenderMeasuredSize extends RenderProxyBox {
  RenderMeasuredSize(this.onSize);

  ValueChanged<Size> onSize;
  Size? _last;

  @override
  void performLayout() {
    super.performLayout();
    if (size == _last) return;
    _last = size;
    final now = size;
    WidgetsBinding.instance.addPostFrameCallback((_) => onSize(now));
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
  const _Messages({required this.emptyTitle, this.emptyDetail, required this.config, required this.padding});

  /// Around the list; on the session screen it clears the floating head and composer.
  final EdgeInsets padding;

  final String emptyTitle;
  final String? emptyDetail;
  final ChatView config;

  @override
  State<_Messages> createState() => _MessagesState();
}

class _MessagesState extends State<_Messages> {
  final _list = GlobalKey<FollowingListState>();

  /// The active board member: a message naming them is marked (B9).
  String? _me;

  /// The server understands `@Everyone` (ticket #40): it is suggested and marked.
  bool get _everyone => context.read<ConnectionBloc>().state.has(everyoneFeature);

  /// [ChatView.revealMessage] was acted on: once, not again on every new message.
  bool _revealed = false;

  /// The server was asked about a [ChatView.revealMessage] this read lacks: once.
  bool _askedWhy = false;

  /// Go to the message a link names once the messages are read. A read that
  /// failed lacks it too, so it comes back here on the next read.
  void _reveal(ChatState state) {
    final target = widget.config.revealMessage;
    if (target == null || _revealed || state.loading) return;
    final message = state.messages.where((m) => m.id == target).firstOrNull;
    if (message == null) {
      if (!_askedWhy) {
        _askedWhy = true;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) unawaited(_sayWhyMissing(target));
        });
      }
      return;
    }
    _revealed = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      unawaited(_goTo(target));
    });
  }

  /// The list could not get there: it is not gone, so it does not say that.
  Future<void> _goTo(String messageId) async {
    final reached = await _list.currentState?.reveal(messageId);
    if (reached == false && mounted) toast(context, 'Could not show that message. Try again.');
  }

  /// Only the server knows a message is gone; anything else is not that.
  Future<void> _sayWhyMissing(String messageId) async {
    String text;
    try {
      await context.read<AureliusRepository>().messageLocation(messageId);
      text = 'Could not show that message. Try again.';
    } on ProtocolException catch (e) {
      text = e.code == ErrorCode.notFound ? 'That message no longer exists.' : e.shown;
    } on Object {
      text = 'Could not show that message. Try again.';
    }
    if (mounted) toast(context, text);
  }

  @override
  Widget build(BuildContext context) {
    final bloc = context.watch<ChatBloc>();
    final state = bloc.state;
    _reveal(state);
    final blocking = context.select<InboxBloc, ApprovalView?>((b) => b.state.blocking(state.conversationId));
    // The group chat: an answer runs in its employee's own chat, so what it asks carries that run.
    final approvals = bloc.group ? context.select<InboxBloc, List<ApprovalView>>((b) => b.state.approvals) : const <ApprovalView>[];
    final mainChats = bloc.group ? context.select<CompanyBloc, List<String>>((b) => [for (final e in b.state.employees) e.conversationId]) : const <String>[];
    final me = context.select<ConnectionBloc, String?>((b) => b.state.user?.username);
    _me = me;
    // Sending a message of your own goes back to the end and follows again.
    return BlocListener<ChatBloc, ChatState>(
      listenWhen: (a, b) => b.unsent.length > a.unsent.length,
      listener: (context, _) => WidgetsBinding.instance.addPostFrameCallback((_) => _list.currentState?.scrollToEnd()),
      child: _build(state, blocking, bloc.group, approvals, mainChats),
    );
  }

  Widget _build(ChatState state, ApprovalView? blocking, bool group, List<ApprovalView> approvals, List<String> mainChats) {
    // On the session screen the floating head and composer stay clear of these too.
    Widget clear(Widget child) => widget.config.floating ? Padding(padding: widget.padding, child: child) : child;
    if (state.loading && state.messages.isEmpty) return clear(const Center(child: CircularProgressIndicator()));
    if (state.messages.isEmpty && state.unsent.isEmpty && state.loadError != null) {
      return clear(EmptyState(
        'This conversation could not be loaded',
        key: const Key('chat-load-error'),
        detail: state.loadError,
        action: OutlinedButton(onPressed: () => context.read<ChatBloc>().add(const ChatRefreshed()), child: const Text('Try again')),
      ));
    }
    if (state.messages.isEmpty && state.unsent.isEmpty) return clear(EmptyState(widget.emptyTitle, detail: widget.emptyDetail));
    final versions = _versions(state.messages);
    // What a run in an employee's own chat asks with no answer streaming here to show it under.
    final asks = [
      for (final a in approvals)
        if (mainChats.contains(a.conversationId) && !state.messages.any((m) => m.isStreaming && m.runId == a.runId)) a,
    ];
    final live = state.messages.indexWhere((m) => m.isStreaming);
    final source = widget.config.source;
    final made = source == null ? const <String, TicketRef>{} : ticketsFromMessages(state.messages);
    // Only the last summary line can be tried again: a later one saved it or failed anew.
    final lastSummary = state.messages.where((m) => m.system?.kind == SystemLineKind.summary || m.system?.kind == SystemLineKind.summaryFailed).lastOrNull?.id;
    bool named(MessageView m) => group && authorOf(m) != _me && mentionsName(m.text, _me);
    final decisions = {for (final m in state.messages) m.id: ?decisionOf(m)};
    final items = <FollowItem>[
      for (final m in state.messages)
        FollowItem(
          m.id,
          m.system != null
              // A system line of a discussion or channel (Phase 2): what happened, never a bubble.
              ? SystemLineView(message: m, ticketId: context.read<ChatBloc>().ticketId, taskProjectId: widget.config.taskProjectId, summarized: m.id != lastSummary)
              : m.isUser
              ? _UserBubble(
                  id: m.id,
                  text: m.text,
                  files: m.files,
                  uploads: m.uploads,
                  queuedId: m.queued ? m.id : null,
                  message: m,
                  author: group ? m.authorUser?.username : null,
                  mentioned: named(m),
                  source: source,
                  ticket: made[m.id],
                )
              // The board member's answer or plan decision on a run: a card under the question.
              : decisions[m.id] != null
                  ? DecisionCard(message: m, decision: decisions[m.id]!)
              // A line the server writes in the group chat itself: a quiet note.
              : isSystemLine(m, inGroup: group)
                  ? _SystemLine(message: m)
                  // An employee's message in the group chat: an answer under its name.
                  : m.update != null && m.update != 'group'
                      ? _TaskUpdate(message: m)
                      : _AssistantMessage(
                          message: m,
                          group: group,
                          mentioned: named(m),
                          source: source,
                          ticket: made[m.id],
                          version: versions[m.id],
                          blocking: !m.isStreaming ? null : group ? approvals.where((a) => a.runId == m.runId).firstOrNull : blocking,
                        ),
        ),
      for (final a in asks) FollowItem('ask-${a.id}', _GroupAsk(approval: a)),
      for (final u in state.unsent) FollowItem('unsent-${u.localId}', _UserBubble(id: 'unsent-${u.localId}', text: u.text, unsent: u)),
    ];
    // Newest at the bottom; the list starts there and holds still when you
    // scroll up while text comes in.
    final list = FollowingList(
      key: _list,
      padding: widget.padding,
      items: items,
      liveFrom: live < 0 ? state.messages.length : live,
      resetKey: state.conversationId,
    );
    // A ticket's discussion marks no names, only `@Everyone` (ticket #40).
    return group && _everyone ? MentionScope(known: const [everyoneName], child: list) : list;
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

/// A note the server writes in the group chat itself: kind `status` there
/// (nobody was woken), kind `system`, or a finished group post without an
/// author. A streaming answer whose employee is not known yet is no note; in a
/// task, `status` stays an update card. As on the desktop.
bool isSystemLine(MessageView m, {required bool inGroup}) {
  if (m.system != null) return true;
  if (m.isUser) return false;
  final unnamed = m.author == null || m.author!.trim().isEmpty;
  return switch (m.update) {
    'system' || 'compacted' => true,
    'status' => inGroup,
    'group' => unnamed && !m.isStreaming,
    _ => false,
  };
}

class _SystemLine extends StatelessWidget {
  const _SystemLine({required this.message});

  final MessageView message;

  @override
  Widget build(BuildContext context) {
    final text = message.text.trim();
    return Padding(
      key: const Key('system-line'),
      padding: const EdgeInsets.fromLTRB(24, 14, 24, 0),
      child: Text(text.isEmpty ? '…' : text, textAlign: TextAlign.center, style: TextStyle(fontSize: 12.5, height: 1.4, color: Tokens.muted)),
    );
  }
}

/// The picker of the fixed reactions (§5): one tap picks one; [mine] are
/// marked, a pick of one of them takes it back.
Future<String?> showReactionPicker(BuildContext context, {Set<String> mine = const {}}) => showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Padding(
          key: const Key('reaction-picker'),
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 12),
          child: Wrap(
            alignment: WrapAlignment.center,
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final emoji in reactionChoices)
                Semantics(
                  button: true,
                  selected: mine.contains(emoji),
                  label: mine.contains(emoji) ? '$emoji, yours' : emoji,
                  excludeSemantics: true,
                  child: InkWell(
                    key: ValueKey('reaction-pick-$emoji'),
                    borderRadius: BorderRadius.circular(10),
                    onTap: () => Navigator.pop(sheet, emoji),
                    child: Container(
                      width: 48,
                      height: 48,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: mine.contains(emoji) ? Tokens.accentHalo : Tokens.surface2,
                        border: Border.all(color: mine.contains(emoji) ? Tokens.accent : Colors.transparent),
                        borderRadius: BorderRadius.circular(10),
                      ),
                      child: Text(emoji, style: TextStyle(fontSize: 22, color: Tokens.text)),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );

/// The reactions under a message (§5): a pill per emoji with its count, yours
/// with the ACCENT_HALO background and an ACCENT border. A tap turns yours on
/// or off and the last pill opens the picker, while the channel is [writable];
/// a long press says who reacted.
class ReactionsRow extends StatelessWidget {
  const ReactionsRow({super.key, required this.message, required this.writable, required this.onToggle, required this.onPick});

  final MessageView message;
  final bool writable;
  final void Function(String emoji, bool on) onToggle;
  final VoidCallback onPick;

  @override
  Widget build(BuildContext context) {
    Widget pill({Key? key, required Widget child, bool mine = false, VoidCallback? onTap, VoidCallback? onLongPress}) => GestureDetector(
          key: key,
          behavior: HitTestBehavior.opaque,
          onTap: onTap,
          onLongPress: onLongPress,
          // The pill stays small; its tap area is at least 32 pt high.
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
              decoration: BoxDecoration(
                color: mine ? Tokens.accentHalo : Tokens.surface2,
                border: Border.all(color: mine ? Tokens.accent : Colors.transparent),
                borderRadius: BorderRadius.circular(10),
              ),
              child: child,
            ),
          ),
        );
    return Padding(
      key: ValueKey('reactions-${message.id}'),
      padding: const EdgeInsets.only(top: 2),
      child: Wrap(spacing: 6, children: [
        for (final r in message.reactions)
          Semantics(
            button: writable,
            label: reactionSemantics(r),
            excludeSemantics: true,
            child: pill(
              key: ValueKey('reaction-${message.id}-${r.emoji}'),
              mine: r.mine,
              onTap: writable ? () => onToggle(r.emoji, !r.mine) : null,
              onLongPress: () => toast(context, reactionWho(r)),
              child: Text('${r.emoji} ${r.count}', style: TextStyle(fontSize: 11.5, color: r.mine ? Tokens.accentText : Tokens.text2, fontWeight: r.mine ? FontWeight.w600 : null)),
            ),
          ),
        if (writable)
          Semantics(
            button: true,
            label: 'Add a reaction',
            excludeSemantics: true,
            child: pill(key: ValueKey('reaction-add-${message.id}'), onTap: onPick, child: Icon(Icons.add_reaction_outlined, size: 14, color: Tokens.text2)),
          ),
      ]),
    );
  }
}

/// Something a run in an employee's own chat asks, in the group chat while
/// no answer of that run streams there.
class _GroupAsk extends StatelessWidget {
  const _GroupAsk({required this.approval});

  final ApprovalView approval;

  @override
  Widget build(BuildContext context) {
    final a = approval;
    return Padding(
      key: ValueKey('group-ask-${a.id}'),
      padding: const EdgeInsets.only(top: 14),
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
        decoration: BoxDecoration(color: Tokens.attentionBg, border: Border.all(color: Tokens.attentionBorder), borderRadius: BorderRadius.circular(8)),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('${a.isQuestion ? 'QUESTION · WAITING FOR YOU' : a.isPlan ? 'PLAN · WAITING FOR YOU' : 'WAITING FOR YOU'} · ${a.requester}', style: Tokens.eyebrow().copyWith(fontSize: 10.5)),
            const SizedBox(height: 6),
            Text(a.title, style: TextStyle(fontSize: 14, color: Tokens.text2)),
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: FilledButton.icon(
                onPressed: () => context.push('/approval/${a.id}'),
                icon: Icon(a.isQuestion ? Icons.help_outline : Icons.pan_tool_outlined, size: 18),
                label: Text(a.isQuestion ? 'Answer' : 'Decide'),
              ),
            ),
          ],
        ),
      ),
    );
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
  const _UserBubble({required this.id, required this.text, this.files = const [], this.uploads = const [], this.unsent, this.queuedId, this.message, this.author, this.mentioned = false, this.source, this.ticket});

  /// The message itself (not for one still on its way), for "Create ticket".
  final MessageView? message;

  /// In a discussion or channel: the board member who wrote it (A-01).
  final String? author;

  /// It names the active board member with `@` (B9).
  final bool mentioned;

  /// Where the chat is: the message can become a ticket (MU2-10).
  final TicketSource? source;

  /// The ticket already made from it ("→ #52").
  final TicketRef? ticket;

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
    final ticketable = source != null && message != null;
    final link = messageLinkable(context, message);
    final editable = messageEditable(context, message);
    if (queuedId == null && !ticketable && !link && !editable) {
      Clipboard.setData(ClipboardData(text: text));
      toast(context, 'Copied.');
      return;
    }
    final bloc = context.read<ChatBloc>();
    final choice = await showGlassMenu<String>(context, [
      if (queuedId != null) const GlassMenuItem('now', 'Send now'),
      const GlassMenuItem('copy', 'Copy'),
      if (link) const GlassMenuItem('link', 'Copy link'),
      if (ticketable) const GlassMenuItem('ticket', 'Create ticket'),
      if (editable) const GlassMenuItem('edit', 'Edit'),
      if (editable) const GlassMenuItem('remove', 'Delete', danger: true),
      if (queuedId != null) const GlassMenuItem('delete', 'Delete', danger: true),
    ]);
    if (!context.mounted) return;
    switch (choice) {
      case 'link':
        await copyMessageLink(context, message!.id);
      case 'ticket':
        await createTicketFrom(context, message!, source!, already: ticket);
      case 'edit':
        await editOwnMessage(context, message!);
      case 'remove':
        await deleteOwnMessage(context, message!);
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
    const end = WrapAlignment.end;
    // Deleted (A6): the place stays, the words, files and copy do not.
    if (message?.isDeleted == true) {
      return Padding(
        padding: const EdgeInsets.only(top: 14, left: 48),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            if (author != null) Padding(padding: const EdgeInsets.only(bottom: 4), child: Text('$author · board member', style: Tokens.monoSmall())),
            DeletedMessageLine(message: message!, author: author ?? message!.authorUser?.username),
            if (ticket != null) TicketPill(ticket: ticket!),
          ],
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(top: 14, left: 48),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          if ((author != null || mentioned))
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Text([if (author != null) '$author · board member', if (mentioned) '@ you'].join(' · '), style: Tokens.monoSmall().copyWith(color: mentioned ? Tokens.accentText : null)),
            ),
          if (text.isNotEmpty || (files.isEmpty && sentFiles.isEmpty && unsent?.images.isEmpty != false)) Collapsible(
            id: id,
            text: text,
            alignment: CrossAxisAlignment.end,
            // Long-press copies the whole message, also while it is folded.
            builder: (context, cut) => GestureDetector(
              onLongPress: () => _longPress(context),
              // Markdown as it was written (AUR-CHAT-001 §4), its @names marked.
              // The board member's words: white on green (design "Aurelius sessiescherm").
              child: Container(
                key: const Key('user-bubble'),
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                decoration: BoxDecoration(
                  color: Tokens.accent.withValues(alpha: muted ? 0.55 : 1),
                  border: mentioned ? Border.all(color: Tokens.accentDeep, width: 1.5) : null,
                  borderRadius: const BorderRadius.only(topLeft: Radius.circular(20), topRight: Radius.circular(20), bottomLeft: Radius.circular(20), bottomRight: Radius.circular(6)),
                ),
                child: LinkedText(cut == null ? text : text.substring(0, cut), scope: chatFileScope(context), style: TextStyle(fontSize: 15, height: 23 / 15, color: Tokens.accentInk)),
              ),
            ),
          ),
          if (messageEditedShown(context, message)) EditedMark(message: message!),
          if (ticket != null) TicketPill(ticket: ticket!),
          if (files.isNotEmpty) MessageFiles(files: files),
          if (sentFiles.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                alignment: end,
                children: [for (final (name, size, path, file) in sentFiles) _UploadChip(name: name, byteLength: size, path: path, file: file)],
              ),
            ),
          if (unsent != null && unsent!.images.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Wrap(spacing: 6, runSpacing: 6, alignment: end, children: [for (final i in unsent!.images) _Thumbnail(file: i.local)]),
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
                alignment: end,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Padding(
                    padding: const EdgeInsets.only(right: 2),
                    child: Text(queuedLine(message?.queuedFor), style: TextStyle(fontSize: 12, color: Tokens.muted)),
                  ),
                  _GlassPill(key: const Key('queued-send-now'), label: 'Send now', accent: true, onTap: () => context.read<ChatBloc>().add(ChatQueuedSentNow(queuedId!))),
                  _GlassPill(key: const Key('queued-delete'), label: 'Delete', danger: true, onTap: () => context.read<ChatBloc>().add(ChatQueuedDeleted(queuedId!))),
                ],
              ),
            ),
          if (unsent?.failed == true)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              // Wraps on a narrow phone or with large text rather than overflowing.
              child: Wrap(
                alignment: end,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Icon(Icons.error_outline, size: 14, color: Tokens.negative),
                  const SizedBox(width: 4),
                  Text(unsent!.secretItems.isEmpty ? 'Not sent' : 'Not sent · looks like a vault secret', style: TextStyle(fontSize: 12, color: Tokens.negative)),
                  if (unsent!.secretItems.isEmpty)
                    TextButton(onPressed: () => context.read<ChatBloc>().add(ChatUnsentRetried(unsent!.localId)), child: const Text('Try again'))
                  else
                    TextButton(
                      key: const Key('unsent-send-anyway'),
                      onPressed: () => sendAnywayAsked(context, context.read<ChatBloc>(), unsent!),
                      child: const Text('Send anyway…'),
                    ),
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
  const _AssistantMessage({required this.message, this.group = false, this.version, this.blocking, this.mentioned = false, this.source, this.ticket});

  final MessageView message;

  /// It names the active board member with `@` (B9).
  final bool mentioned;

  /// Where the chat is: the message can become a ticket (MU2-10).
  final TicketSource? source;

  /// The ticket already made from it ("→ #52").
  final TicketRef? ticket;

  /// In the Team chat: the sender shows with their role.
  final bool group;
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
            if (messageLinkable(context, message))
              ListTile(
                key: const Key('message-copy-link'),
                leading: const Icon(Icons.link),
                title: const Text('Copy link'),
                onTap: () {
                  Navigator.pop(sheet);
                  copyMessageLink(context, message.id);
                },
              ),
            if (source != null && !message.isStreaming)
              ListTile(
                key: const Key('message-create-ticket'),
                leading: const Icon(CupertinoIcons.square_split_2x1),
                title: const Text('Create ticket'),
                onTap: () {
                  Navigator.pop(sheet);
                  createTicketFrom(context, message, source!, already: ticket);
                },
              ),
            if (target != null && !message.isStreaming && !bloc.group)
              ListTile(
                leading: const Icon(Icons.refresh),
                title: const Text('Answer again'),
                onTap: () {
                  bloc.add(ChatReplyRetried(target));
                  Navigator.pop(sheet);
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
    final author = message.author?.trim();
    final role = group && author != null && author.isNotEmpty ? context.select<CompanyBloc, String?>((b) => employeeRole(b.state, author)) : null;
    // Several employees in one conversation: who speaks stands on top. Else
    // who answered and on which model sit beside Copy, under the answer.
    final header = [
      if (group && author != null && author.isNotEmpty) withRole(author, role),
      // A group chat answer: the mode the employee answered in.
      if (message.mode case final mode?) '${mode.label} mode',
      if (version != null) 'version ${version!.$1}/${version!.$2}',
      if (mentioned) '@ you',
    ].join(' · ');
    final footer = answerFooter(author: group ? null : author, producedBy: message.producedBy);
    return Padding(
      padding: const EdgeInsets.only(top: 14),
      child: GestureDetector(
        onLongPress: () => _actions(context),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (header.isNotEmpty) Padding(padding: const EdgeInsets.only(bottom: 4), child: Text(header, style: Tokens.monoSmall().copyWith(color: mentioned ? Tokens.accentText : null))),
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
            if (message.text.isNotEmpty && !message.isStreaming)
              Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Row(children: [
                  Transform.translate(offset: const Offset(-6, 0), child: CopyButton(key: const Key('copy-assistant'), text: message.text, tooltip: 'Copy message')),
                  if (footer.isNotEmpty) Flexible(child: Text(footer, key: const Key('answer-footer'), maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, color: Tokens.label2))),
                ]),
              ),
            if (ticket != null) TicketPill(ticket: ticket!),
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
          [if (p.compacting == true) compactingLabel else ?step, Fmt.progress(p.startedAtMs, p.outputTokens)].where((t) => t.isNotEmpty).join(' · '),
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

class _Composer extends StatefulWidget {
  const _Composer({this.draft, required this.config});

  /// Told what is typed, so quick replies can step aside.
  final ValueNotifier<String>? draft;
  final ChatView config;

  @override
  State<_Composer> createState() => _ComposerState();
}

class _ComposerState extends State<_Composer> with WidgetsBindingObserver {
  final _own = TextEditingController();
  late final TextEditingController _text = widget.config.composerText ?? _own;
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
    // A draft kept from before (a sign-out in between, R3 M1) comes back.
    final repository = context.read<AureliusRepository>();
    _slot = widget.config.draftSlot ?? _chatSlot(context.read<ChatBloc>());
    final kept = _slot == null ? null : repository.draft(_slot!);
    if (kept != null && _text.text.isEmpty) _text.text = kept;
    _keep = (text) {
      if (_slot != null) repository.keepDraft(_slot!, text);
    };
    _text.addListener(_drafted);
    // Quick replies step aside for it too, after this build.
    if (kept != null) WidgetsBinding.instance.addPostFrameCallback((_) => mounted ? widget.draft?.value = _text.text : null);
  }

  String? _slot;
  late final void Function(String text) _keep;

  static String? _chatSlot(ChatBloc bloc) => switch (bloc) {
        ChatBloc(:final ticketId?) => 'ticket:$ticketId',
        ChatBloc(state: ChatState(:final conversationId?)) => 'conversation:$conversationId',
        ChatBloc(:final employeeId?) => 'employee:$employeeId',
        ChatBloc(:final projectId?) => 'project:$projectId',
        _ => null,
      };

  /// Every change is kept at once; a send clears the field, which forgets it.
  void _drafted() {
    widget.draft?.value = _text.text;
    _keep(_text.text);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _checkClipboard();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _text.removeListener(_drafted);
    _own.dispose();
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
    final items = _clipboardImage ? withPasteImage(field.contextMenuButtonItems, _pasteImage) : field.contextMenuButtonItems;
    return AdaptiveTextSelectionToolbar.buttonItems(anchors: field.contextMenuAnchors, buttonItems: items);
  }

  /// Sends the message. With [now] it interrupts a running answer; without, it
  /// waits behind it (queued).
  void _send({bool now = false}) {
    final text = _text.text;
    final bloc = context.read<ChatBloc>();
    if (text.trim().isEmpty && bloc.state.images.isEmpty && bloc.state.files.isEmpty) return;
    // A name that is nobody (or archived) wakes no one: said, never silently (MU2-04 AC5).
    if (bloc.group) {
      final known = withEveryone([for (final p in mentionables(context.read<CompanyBloc>().state)) p.$1, ...bloc.state.boardMembers], enabled: context.read<ConnectionBloc>().state.has(everyoneFeature));
      final nobody = unknownMentions(text, known);
      if (nobody.isNotEmpty) toast(context, '${nobody.map((n) => '@$n').join(', ')} ${nobody.length == 1 ? 'is' : 'are'} nobody here: no one is woken for ${nobody.length == 1 ? 'it' : 'them'}.');
    }
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
    await _addImage(data, pastedImageExtension(content.mimeType));
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

  /// The photo and file rows of "+": only a ticket's discussion keeps its attachments.
  bool _canAttach(ChatBloc bloc) => bloc.ticketId != null;

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
            if (_canAttach(bloc)) ...[
              ListTile(enabled: !state.uploading, leading: const Icon(Icons.photo_camera_outlined), title: const Text('Take a photo'), onTap: () => Navigator.pop(sheet, 'camera')),
              ListTile(enabled: !state.uploading, leading: const Icon(Icons.photo_library_outlined), title: const Text('Photo library'), onTap: () => Navigator.pop(sheet, 'gallery')),
              ListTile(enabled: !state.uploading, leading: const Icon(Icons.description_outlined), title: const Text('File'), onTap: () => Navigator.pop(sheet, 'file')),
            ],
            // In the group chat each employee answers with its own settings.
            if (!bloc.group) ...[
              if (_canAttach(bloc)) const Divider(height: 1),
              tool(sheet, 'browser', Icons.language, 'Browser', 'Let this agent use the browser.', state.browserAllowed),
              tool(sheet, 'screen', Icons.screenshot_monitor, 'Screen', 'Screenshots of the PC without asking.', state.screenAllowed),
            ],
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

  /// On the session screen the field, the clip and Send sit in one white pill.
  Widget _inputShell(Widget row) {
    if (!widget.config.floating) return row;
    return Container(
      key: const Key('composer-pill'),
      padding: const EdgeInsets.all(6),
      decoration: BoxDecoration(
        color: Tokens.cell,
        border: Border.all(color: Tokens.sep),
        // Fully round: concentric with the 40 pt clip and Send at a 6 pt inset.
        borderRadius: BorderRadius.circular(999),
        boxShadow: const [BoxShadow(color: Color(0x0A000000), blurRadius: 2, offset: Offset(0, 1)), BoxShadow(color: Color(0x0D000000), blurRadius: 16, offset: Offset(0, 6))],
      ),
      child: row,
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<ChatBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final readOnly = state.history || (link.isOnline && !link.can('chat'));
    final group = context.read<ChatBloc>().group;
    final floating = widget.config.floating;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): _sendDefault,
        const SingleActivator(LogicalKeyboardKey.enter, control: true): _sendDefault,
        const SingleActivator(LogicalKeyboardKey.enter, alt: true): _queue,
        const SingleActivator(LogicalKeyboardKey.keyV, meta: true, shift: true): _pasteImage,
        const SingleActivator(LogicalKeyboardKey.keyV, control: true, shift: true): _pasteImage,
        if (!group) const SingleActivator(LogicalKeyboardKey.tab, shift: true): () => context.read<ChatBloc>().add(ChatModeSelected(state.mode.next)),
      },
      // The whole composer counts as the field: sending or attaching keeps
      // the keyboard up; a tap on the messages above closes it.
      child: TextFieldTapRegion(
        child: Container(
        decoration: floating ? null : BoxDecoration(color: Tokens.plain, border: Border(top: BorderSide(color: Tokens.sep, width: 0.5))),
        padding: floating ? EdgeInsets.fromLTRB(16, 0, 16, 12 + MediaQuery.paddingOf(context).bottom) : EdgeInsets.fromLTRB(12, 8, 12, 8 + MediaQuery.paddingOf(context).bottom),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // The session screen: mode and model in glass pills above the field.
            if (floating && !group) ...[
              Align(
                alignment: Alignment.centerLeft,
                child: _FloatingChips(state: state, readOnly: readOnly, pickMode: widget.config.modeFixed ? null : () => _pickMode(state.mode), toggleBrowser: () => _toggleBrowser(state), toggleScreen: () => _toggleScreen(state)),
              ),
              const SizedBox(height: 10),
            ],
            // Team chat: who the `@name` being typed can be, with their role.
            if (group)
              ValueListenableBuilder<TextEditingValue>(
                valueListenable: _text,
                builder: (context, value, _) {
                  // Employees first, then the other board members (B9: marked, not woken).
                  final me = context.read<ConnectionBloc>().state.user?.username;
                  final people = [
                    ...mentionables(context.read<CompanyBloc>().state),
                    for (final u in state.boardMembers)
                      if (u != me) (u, 'board member'),
                  ];
                  final everyone = context.read<ConnectionBloc>().state.has(everyoneFeature);
                  final found = mentionSuggestions(value.text, withEveryone([for (final p in people) p.$1], enabled: everyone));
                  if (found == null) return const SizedBox.shrink();
                  return MentionRow(
                    found: found.$2,
                    roles: {for (final p in people) p.$1: p.$2, everyoneName: everyoneRole},
                    onPick: (name) {
                      final text = completeMention(value.text, found.$1, name);
                      _text.value = TextEditingValue(text: text, selection: TextSelection.collapsed(offset: text.length));
                      _focus.requestFocus();
                    },
                  );
                },
              ),
            // Before sending: whom an `@Everyone` in the text reaches (ticket #40).
            if (group && context.read<ConnectionBloc>().state.has(everyoneFeature))
              EveryoneAudienceLine(
                text: _text,
                channel: state.conversationId,
                load: (channel) => context.read<AureliusRepository>().discussionAudienceOf(channel: channel),
              ),
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
            _inputShell(Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                if (_canAttach(context.read<ChatBloc>()) || !context.read<ChatBloc>().group)
                  floating
                      // A flat filled circle: floating glass cast its 28 pt shadow inside the pill,
                      // so the clip looked lower and further out than the pill's edge.
                      ? DecoratedBox(
                          key: const Key('composer-clip'),
                          decoration: BoxDecoration(color: Tokens.cell2, shape: BoxShape.circle),
                          child: SizedBox.square(
                            dimension: 40,
                            child: IconButton(
                              key: const Key('composer-plus'),
                              tooltip: 'Attach file',
                              padding: EdgeInsets.zero,
                              onPressed: readOnly ? null : () => _attach(state),
                              icon: Icon(CupertinoIcons.paperclip, size: 20, color: Tokens.ink2),
                            ),
                          ),
                        )
                      : IconButton(
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
                      allowedMimeTypes: pastedImageMimeTypes,
                      onContentInserted: _insert,
                    ),
                    // Floating: a 20 pt line plus 2 × 10 padding makes one line as high as the 40 pt buttons.
                    style: floating ? TextStyle(fontSize: 16, height: 1.25, color: Tokens.label) : null,
                    decoration: InputDecoration(
                      hintText: state.history ? 'Read-only: an earlier chat' : readOnly ? 'Read-only: this phone may not chat' : widget.config.hint ?? (group ? 'Message the team · @Name asks someone' : 'Message'),
                      hintStyle: floating ? TextStyle(fontSize: 16, height: 1.25, color: Tokens.label2) : null,
                      // A long hint would wrap and make the empty field taller than the buttons.
                      hintMaxLines: 1,
                      isDense: true,
                      fillColor: floating ? Colors.transparent : Tokens.cell2,
                      contentPadding: EdgeInsets.symmetric(horizontal: floating ? 12 : 16, vertical: floating ? 10 : 11),
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
                    final side = floating ? 40.0 : 42.0;
                    final style = IconButton.styleFrom(
                      backgroundColor: Tokens.accent.withValues(alpha: floating ? 0.92 : 1),
                      foregroundColor: Tokens.accentInk,
                      fixedSize: Size.square(side),
                      minimumSize: Size.square(side),
                      // In the pill the 48 pt tap margin made the pill taller than the clip and Send.
                      tapTargetSize: floating ? MaterialTapTargetSize.shrinkWrap : null,
                    );
                    // The group chat: several may answer at once; each answer has its own Stop.
                    final runId = group ? null : running?.runId;
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
                      message: running != null && !group ? 'Send now: interrupt the answer (⌘↩). Hold to send after it (⌥↩)' : 'Send (⌘↩)',
                      triggerMode: TooltipTriggerMode.manual,
                      child: GestureDetector(
                        onLongPress: readOnly || running == null || empty || group ? null : _queue,
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
            )),
            if (widget.config.composerActions case final actions?) actions(context, _text),
            if (!group && !floating) const SizedBox(height: 6),
            // Mode and model, then what is switched on under "+" (tap to switch it off).
            if (!group && !floating) SingleChildScrollView(
              key: const Key('composer-chips'),
              scrollDirection: Axis.horizontal,
              child: Row(
                children: [
                  if (!widget.config.modeFixed) ...[
                    ActionChip(
                      avatar: Icon(Icons.tune, size: 16, color: state.mode == AgentMode.auto ? Tokens.warning : Tokens.text2),
                      label: Text(state.mode.label, style: TextStyle(color: state.mode == AgentMode.auto ? Tokens.warning : Tokens.text2)),
                      onPressed: readOnly ? null : () => _pickMode(state.mode),
                    ),
                    const SizedBox(width: 6),
                  ],
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

/// The session screen's mode and model pills above the composer, then what
/// is switched on under the clip (tap to switch it off).
class _FloatingChips extends StatelessWidget {
  const _FloatingChips({required this.state, required this.readOnly, required this.pickMode, required this.toggleBrowser, required this.toggleScreen});

  final ChatState state;
  final bool readOnly;
  /// Null: the mode is fixed (a design session), so no mode pill.
  final VoidCallback? pickMode;
  final VoidCallback toggleBrowser;
  final VoidCallback toggleScreen;

  @override
  Widget build(BuildContext context) {
    final run = _watchRun(context);
    final history = state.history;
    final overridden = run?.overridden ?? false;
    final model = run == null || run.effectiveModel == null && !overridden ? 'Model' : runLabel(run.effectiveModel, run.effectiveEffort, effortShown: run.effortShown);
    return SingleChildScrollView(
      key: const Key('composer-chips'),
      scrollDirection: Axis.horizontal,
      padding: const EdgeInsets.symmetric(horizontal: 2),
      child: Row(children: [
        if (pickMode case final pick?) ...[
          _ChromePill(
            key: const Key('composer-mode'),
            leading: Icon(Icons.tune, size: 14, color: state.mode == AgentMode.auto ? Tokens.warning : Tokens.ink2),
            label: state.mode.label,
            onTap: readOnly ? null : pick,
          ),
          const SizedBox(width: 6),
        ],
        _ChromePill(
          key: const Key('conversation-model-chip'),
          label: model,
          accent: overridden,
          trailing: Icon(CupertinoIcons.chevron_down, size: 12, color: overridden ? Tokens.accentText : Tokens.ink2),
          onTap: history ? null : () => _showConversationModelSheet(context),
        ),
        if (state.browserAllowed) ...[
          const SizedBox(width: 6),
          _ChromePill(key: const Key('browser-chip'), leading: Icon(Icons.language, size: 14, color: Tokens.warning), label: 'Browser', trailing: Icon(Icons.close, size: 12, color: Tokens.ink2), onTap: readOnly ? null : toggleBrowser),
        ],
        if (state.screenAllowed) ...[
          const SizedBox(width: 6),
          _ChromePill(key: const Key('screen-chip'), leading: Icon(Icons.screenshot_monitor, size: 14, color: Tokens.warning), label: 'Screen', trailing: Icon(Icons.close, size: 12, color: Tokens.ink2), onTap: readOnly ? null : toggleScreen),
        ],
      ]),
    );
  }
}

/// A 30 pt glass capsule with a label (the session screen's mode and model).
class _ChromePill extends StatelessWidget {
  const _ChromePill({super.key, required this.label, this.onTap, this.leading, this.trailing, this.accent = false});

  final String label;
  final VoidCallback? onTap;
  final Widget? leading;
  final Widget? trailing;
  final bool accent;

  @override
  Widget build(BuildContext context) => Glass(
        radius: 15,
        child: Material(
          type: MaterialType.transparency,
          child: InkWell(
            onTap: onTap,
            child: Container(
              height: 30,
              padding: const EdgeInsets.symmetric(horizontal: 10),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                if (leading case final leading?) ...[leading, const SizedBox(width: 5)],
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 200),
                  child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: accent ? Tokens.accentText : Tokens.ink2)),
                ),
                if (trailing case final trailing?) ...[const SizedBox(width: 4), trailing],
              ]),
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

/// "Create ticket" on a message (MU2-10 AC1): "New ticket" opens filled in from
/// it. A message a ticket was made from asks first (ui-ontwerp §4).
Future<void> createTicketFrom(BuildContext context, MessageView message, TicketSource source, {TicketRef? already}) async {
  if (already != null) {
    final again = await confirm(context, title: '${already.label} was already created from this message. Create another?', action: 'Create another');
    if (!again || !context.mounted) return;
  }
  context.push('/board/new', extra: draftFromMessage(message, source, author: authorOf(message)));
}

/// "→ #52" under a message a ticket was made from; a tap opens the ticket.
class TicketPill extends StatelessWidget {
  const TicketPill({super.key, required this.ticket});

  final TicketRef ticket;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(top: 4),
        child: Semantics(
          button: true,
          label: 'Open ticket ${ticket.number ?? ''}'.trim(),
          excludeSemantics: true,
          child: _GlassPill(key: ValueKey('ticket-pill-${ticket.ticketId}'), label: '→ ${ticket.label}', accent: true, onTap: () => context.push('/ticket/${ticket.ticketId}')),
        ),
      );
}

/// A system line of a discussion or channel (`MessageView.system`, Phase 2):
/// one quiet centred line that never wakes anyone (MU2-03 AC5). A line about a
/// task opens it; the plan and the report that wait for a board member are a
/// card with their action; a failed summary has Try again (MU2-12 AC4).
class SystemLineView extends StatelessWidget {
  const SystemLineView({super.key, required this.message, this.ticketId, this.taskProjectId, this.summarized = false});

  final MessageView message;

  /// The discussion's ticket (a line may name its own).
  final String? ticketId;

  /// The project of the tasks the lines name.
  final String? taskProjectId;

  /// A later line says the summary went to memory (or failed again): Try again is gone.
  final bool summarized;

  @override
  Widget build(BuildContext context) {
    final line = message.system!;
    final plain = message.text.trim().isEmpty ? '…' : message.text.trim();
    final text = plain;
    final ticket = line.ticketId ?? ticketId;
    final project = taskProjectId;
    return switch (line.kind) {
      SystemLineKind.plan when ticket != null => _FlowCard(
          key: ValueKey('system-plan-${message.id}'),
          eyebrow: '◆ PLAN${line.from == null ? '' : ' · ${line.from!.toUpperCase()}'}',
          text: plain,
          action: 'Review plan',
          onTap: () => context.push('/ticket/$ticket/plan'),
        ),
      SystemLineKind.review when ticket != null => _FlowCard(
          key: ValueKey('system-review-${message.id}'),
          eyebrow: '◆ REVIEW · REPORT${line.from == null ? '' : ' FROM ${line.from!.toUpperCase()}'}',
          text: plain,
          action: 'Report ›',
          onTap: () => context.push('/ticket/$ticket/report'),
        ),
      SystemLineKind.summaryFailed => _Line(
          key: ValueKey('system-summary-failed-${message.id}'),
          message: message,
          text: '⚠ $text',
          color: Tokens.warning,
          action: summarized || ticket == null ? null : ('Try again', () => retrySummary(context, ticket)),
        ),
      SystemLineKind.ticketCreated when line.ticketId != null => _Line(message: message, text: text, onTap: () => context.push('/ticket/${line.ticketId}')),
      SystemLineKind.reported when line.taskId != null && project != null =>
        _Line(message: message, text: '$text · Report ›', onTap: () => context.push('/project/$project/task/${line.taskId}')),
      SystemLineKind.delegated when line.taskId != null && project != null => _Line(message: message, text: text, onTap: () => context.push('/project/$project/task/${line.taskId}')),
      _ => _Line(message: message, text: text),
    };
  }
}

/// `ticket.retry_summary` (MU2-12 AC4): the summary of a ticket on Done again.
/// A server without the engine answers `unknown_method`: it says so.
Future<bool> retrySummary(BuildContext context, String ticketId) async {
  final repository = context.read<AureliusRepository>();
  final messenger = ScaffoldMessenger.maybeOf(context);
  void show(String text) => messenger
    ?..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(text)));
  try {
    await repository.write(ticketRetrySummary(ticketId: ticketId), retry: false);
    show('Trying the summary again.');
    return true;
  } on NotSentException catch (e) {
    show(e.toString());
  } on DisconnectedException catch (e) {
    show(e.message);
  } on Object catch (e) {
    show(summaryRetryFailure(e));
  }
  return false;
}

class _Line extends StatelessWidget {
  const _Line({super.key, required this.message, required this.text, this.onTap, this.color, this.action});

  final MessageView message;
  final String text;
  final VoidCallback? onTap;
  final Color? color;
  final (String, VoidCallback)? action;

  @override
  Widget build(BuildContext context) {
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final line = Text(text, textAlign: TextAlign.center, style: TextStyle(fontSize: 12.5, height: 1.4, color: color ?? (onTap != null ? Tokens.text2 : Tokens.muted), decoration: onTap != null ? TextDecoration.underline : null, decorationColor: Tokens.border));
    return Padding(
      key: key == null ? const Key('system-line') : null,
      padding: const EdgeInsets.fromLTRB(24, 14, 24, 0),
      child: Column(children: [
        Semantics(
          label: systemLineLabel(message),
          button: onTap != null,
          excludeSemantics: true,
          child: onTap == null ? line : InkWell(onTap: onTap, borderRadius: BorderRadius.circular(6), child: Padding(padding: const EdgeInsets.symmetric(vertical: 4), child: line)),
        ),
        if (action != null)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: OutlinedButton(key: const Key('system-line-action'), onPressed: online ? action!.$2 : null, child: Text(action!.$1)),
          ),
      ]),
    );
  }
}

/// The plan or the report in the stream, with what opens it.
class _FlowCard extends StatelessWidget {
  const _FlowCard({super.key, required this.eyebrow, required this.text, required this.action, required this.onTap});

  final String eyebrow;
  final String text;
  final String action;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(top: 14),
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
          decoration: BoxDecoration(color: Tokens.attentionBg, border: Border.all(color: Tokens.attentionBorder), borderRadius: BorderRadius.circular(8)),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(eyebrow, style: Tokens.eyebrow().copyWith(fontSize: 10.5)),
            const SizedBox(height: 6),
            Text(text, maxLines: 3, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 14, color: Tokens.text2)),
            Padding(padding: const EdgeInsets.only(top: 8), child: FilledButton(onPressed: onTap, child: Text(action))),
          ]),
        ),
      );
}
