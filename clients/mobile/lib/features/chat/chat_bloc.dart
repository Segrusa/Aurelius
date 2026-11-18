import 'dart:async';
import 'dart:io';

import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:uuid/uuid.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart' show reactedLocally;
import '../../data/commands.dart';
import '../../data/discussion.dart';
import '../../data/mode_memory.dart';
import '../../data/vault.dart' show secretCheckNoSendAnyway;
import '../../data/views.dart';

sealed class ChatEvent {
  const ChatEvent();
}

class ChatRefreshed extends ChatEvent {
  const ChatRefreshed();
}

/// Whether the Team chat is on screen. Reading a channel marks it read on the
/// server (`group_chat.get`, schema 20): hidden, changes only mark it stale and
/// it is read again once it shows, so its unread and mentions stay in the badge.
class ChatShown extends ChatEvent {
  const ChatShown(this.visible);

  final bool visible;
}

/// The channel can be written in again (AUR-CHAT-001 §8 Reopen): its
/// ticket left Done, so `channel_archived` no longer holds.
class ChatReopened extends ChatEvent {
  const ChatReopened();
}

/// The board member's own reaction [emoji] on [messageId], added ([on]) or
/// taken back (feature `chat_reactions`): shown at once, undone when refused.
class ChatReacted extends ChatEvent {
  const ChatReacted(this.messageId, this.emoji, {required this.on});

  final String messageId;
  final String emoji;
  final bool on;
}

class _AppendReceived extends ChatEvent {
  const _AppendReceived(this.append);

  final StreamAppend append;
}

/// Group chat: a second passed while answers stream in (see [ChatBloc._tick]).
class _LiveTicked extends ChatEvent {
  const _LiveTicked();
}

class ChatMessageSent extends ChatEvent {
  const ChatMessageSent(this.text, {this.now = false});

  final String text;

  /// "Send now": interrupt the running answer instead of waiting for it.
  final bool now;
}

/// Resend an unsent message with its original idempotency key (applied at most once).
class ChatUnsentRetried extends ChatEvent {
  const ChatUnsentRetried(this.localId);

  final String localId;
}

/// Drop an unsent message: after a failure, but also while it is still
/// sending or waiting to be delivered.
class ChatUnsentDiscarded extends ChatEvent {
  const ChatUnsentDiscarded(this.localId);

  final String localId;
}

/// Delete a message that waits behind a running answer (optimistic).
class ChatQueuedDeleted extends ChatEvent {
  const ChatQueuedDeleted(this.messageId);

  final String messageId;
}

/// Stop what runs ahead of a waiting message and answer it at once.
class ChatQueuedSentNow extends ChatEvent {
  const ChatQueuedSentNow(this.messageId);

  final String messageId;
}

class ChatStopRequested extends ChatEvent {
  const ChatStopRequested(this.runId);

  final String runId;
}

class ChatReplyRetried extends ChatEvent {
  const ChatReplyRetried(this.userMessageId);

  final String userMessageId;
}

class ChatModeSelected extends ChatEvent {
  const ChatModeSelected(this.mode);

  final AgentMode mode;
}

/// Allow or forbid the agent to use the user's own browser.
class ChatBrowserToggled extends ChatEvent {
  const ChatBrowserToggled();
}

/// Model and effort for this conversation only (an override); both `null`
/// is back to the employee's default.
class ChatRunSelected extends ChatEvent {
  const ChatRunSelected({this.model, this.effort});

  final String? model;
  final String? effort;
}

class ChatScreenToggled extends ChatEvent {
  const ChatScreenToggled();
}

class ChatFileAttached extends ChatEvent {
  const ChatFileAttached(this.file, this.name);

  final File file;
  final String name;
}

/// An image for the next message (photographed, picked or pasted): uploaded
/// now, sent with the message.
class ChatImageAdded extends ChatEvent {
  const ChatImageAdded(this.file, this.name);

  final File file;
  final String name;
}

/// Several photos or files picked at once. Each one over 25 MB, or beyond the
/// per-message limit, is left out and named in one notice; the rest is added.
class ChatPicked extends ChatEvent {
  const ChatPicked(this.picked, {this.images = false});

  final List<(File, String)> picked;
  final bool images;
}

class ChatImageRemoved extends ChatEvent {
  const ChatImageRemoved(this.image);

  final PendingImage image;
}

class ChatFileRemoved extends ChatEvent {
  const ChatFileRemoved(this.file);

  final PendingFile file;
}

class ChatAttachmentRemoved extends ChatEvent {
  const ChatAttachmentRemoved(this.attachmentId);

  final String attachmentId;
}

class ChatNoticeShown extends ChatEvent {
  const ChatNoticeShown();
}

/// "Send anyway" after `looks_like_secret` (B10): send the message again with
/// `confirm_secret`; the PC stores it with the value redacted.
class ChatSecretConfirmed extends ChatEvent {
  const ChatSecretConfirmed(this.localId);

  final String localId;
}

/// The B10 question was answered or dismissed.
class ChatSecretCheckShown extends ChatEvent {
  const ChatSecretCheckShown();
}

/// An uploaded image waiting to go with the next message.
class PendingImage extends Equatable {
  const PendingImage({required this.local, required this.path, required this.name});

  /// The file on this phone (for the thumbnail).
  final File local;

  /// Where the upload landed on the PC.
  final String path;
  final String name;

  @override
  List<Object?> get props => [local.path, path, name];
}

/// An uploaded file waiting to go with the next message to an employee.
class PendingFile extends Equatable {
  const PendingFile({required this.path, required this.name, required this.byteLength});

  /// Where the upload landed on the PC.
  final String path;
  final String name;
  final int byteLength;

  @override
  List<Object?> get props => [path, name, byteLength];
}

/// A message typed on this phone that the PC has not confirmed yet.
class UnsentMessage extends Equatable {
  const UnsentMessage({required this.localId, required this.idempotencyKey, required this.text, this.images = const [], this.files = const [], this.interrupt = false, this.failed = false, this.delivered = false, this.secretItems = const [], this.confirmSecret = false});

  final String localId;
  final String idempotencyKey;
  final String text;
  final List<PendingImage> images;
  final List<PendingFile> files;

  /// "Send now": the PC stops the running answer with it (kept for a resend).
  final bool interrupt;
  final bool failed;

  /// Committed on the PC; shown until the next snapshot contains it.
  final bool delivered;

  /// The PC refused it with `looks_like_secret` (B10): the names of the vault
  /// items whose value it holds. Never a value.
  final List<String> secretItems;

  /// The board member chose "Send anyway": it goes with `confirm_secret` and
  /// the PC stores it with the value redacted.
  final bool confirmSecret;

  UnsentMessage asFailed(bool value) =>
      UnsentMessage(localId: localId, idempotencyKey: idempotencyKey, text: text, images: images, files: files, interrupt: interrupt, failed: value, confirmSecret: confirmSecret);

  UnsentMessage asDelivered() => UnsentMessage(localId: localId, idempotencyKey: idempotencyKey, text: text, images: images, files: files, interrupt: interrupt, delivered: true, confirmSecret: confirmSecret);

  /// Refused as holding [items]' values; it waits for the board member's choice.
  UnsentMessage asSecret(List<String> items) =>
      UnsentMessage(localId: localId, idempotencyKey: idempotencyKey, text: text, images: images, files: files, interrupt: interrupt, failed: true, secretItems: items);

  /// "Send anyway": a new idempotency key, since the request differs from the refused one.
  UnsentMessage asConfirmed(String key) => UnsentMessage(localId: localId, idempotencyKey: key, text: text, images: images, files: files, interrupt: interrupt, confirmSecret: true);

  @override
  List<Object?> get props => [localId, idempotencyKey, text, images, files, interrupt, failed, delivered, secretItems, confirmSecret];
}

class ChatState extends Equatable {
  const ChatState({
    this.loading = true,
    this.conversationId,
    this.messages = const [],
    this.attachments = const [],
    this.mode = AgentMode.ask,
    this.browserAllowed = false,
    this.screenAllowed = false,
    this.runModel,
    this.runEffort,
    this.history = false,
    this.unsent = const [],
    this.images = const [],
    this.files = const [],
    this.uploading = false,
    this.notice,
    this.loadError,
    this.progress = const {},
    this.boardMembers = const [],
    this.secretCheck,
    this.closed = false,
  });

  final bool loading;

  /// `null` for a new conversation until the first message creates it.
  final String? conversationId;
  final List<MessageView> messages;
  final List<AttachmentView> attachments;
  final AgentMode mode;
  final bool browserAllowed;
  final bool screenAllowed;

  /// Model and effort chosen for this conversation alone (`MessagesSnapshot.run`);
  /// `null` follows the employee's default.
  final String? runModel;
  final String? runEffort;

  /// The conversation runs on its own model or effort.
  bool get overridden => runModel != null || runEffort != null;

  /// An older employee chat converted from protocol 2: history only, it
  /// takes no messages (`MessagesSnapshot.read_only`).
  final bool history;
  final List<UnsentMessage> unsent;

  /// Images for the next message.
  final List<PendingImage> images;

  /// Files for the next message to an employee.
  final List<PendingFile> files;
  final bool uploading;

  /// One-off message for a snackbar (errors, confirmations).
  final String? notice;

  /// Why the messages could not be loaded (such as a reply too large to
  /// send), shown in place of them while there are none; cleared by a load.
  final String? loadError;

  /// Answers still streaming: since when they run and their tokens so far.
  final Map<String, RunProgress> progress;

  /// The board members' names, for `@` in a discussion or channel (B9: they
  /// are marked, not woken). Empty elsewhere and when they could not be read.
  final List<String> boardMembers;

  /// The unsent message whose `looks_like_secret` waits for the board
  /// member's choice (B10): a one-off, like [notice].
  final String? secretCheck;

  /// A write here was refused with `channel_archived` (feature
  /// `ticket_channels`): the channel can only be read, the read-only bar
  /// shows in place of the composer until [ChatReopened].
  final bool closed;

  /// The run that is streaming now, if any.
  MessageView? get streaming {
    for (final m in messages.reversed) {
      if (!m.isUser && m.isStreaming) return m;
    }
    return null;
  }

  /// A message waits for an answer that has not started yet: the last one
  /// is the user's (or a task brief/finding the employee answers) and no answer
  /// streams. Its run may be held by the server's load.
  bool get awaitingStart {
    if (streaming != null) return false;
    if (unsent.any((u) => u.delivered)) return true;
    for (final m in messages.reversed) {
      if (m.isUser) return true;
      return m.update == 'brief' || m.update == 'finding' || m.update == 'message';
    }
    return false;
  }

  ChatState copyWith({
    bool? loading,
    String? conversationId,
    List<MessageView>? messages,
    List<AttachmentView>? attachments,
    AgentMode? mode,
    bool? browserAllowed,
    bool? screenAllowed,
    (String?, String?)? run,
    bool? history,
    List<UnsentMessage>? unsent,
    List<PendingImage>? images,
    List<PendingFile>? files,
    bool? uploading,
    String? notice,
    String? loadError,
    bool clearLoadError = false,
    Map<String, RunProgress>? progress,
    List<String>? boardMembers,
    String? secretCheck,
    bool? closed,
  }) =>
      ChatState(
        loading: loading ?? this.loading,
        conversationId: conversationId ?? this.conversationId,
        messages: messages ?? this.messages,
        attachments: attachments ?? this.attachments,
        mode: mode ?? this.mode,
        browserAllowed: browserAllowed ?? this.browserAllowed,
        screenAllowed: screenAllowed ?? this.screenAllowed,
        runModel: run == null ? runModel : run.$1,
        runEffort: run == null ? runEffort : run.$2,
        history: history ?? this.history,
        unsent: unsent ?? this.unsent,
        images: images ?? this.images,
        files: files ?? this.files,
        uploading: uploading ?? this.uploading,
        notice: notice,
        loadError: clearLoadError ? null : loadError ?? this.loadError,
        progress: progress ?? this.progress,
        boardMembers: boardMembers ?? this.boardMembers,
        secretCheck: secretCheck,
        closed: closed ?? this.closed,
      );

  @override
  List<Object?> get props => [loading, conversationId, messages, attachments, mode, browserAllowed, screenAllowed, runModel, runEffort, history, unsent, images, files, uploading, notice, loadError, progress, boardMembers, secretCheck, closed];
}

/// One conversation: an employee's task conversation when [employeeId] is set
/// (messages then go to the employee), a project conversation when [projectId]
/// is set (messages go to `project_conversation.send`), or the company's group
/// chat with [ChatBloc.group] (`group_chat.get` / `group_chat.send`).
/// [session]: a work session's conversation (feature `work_sessions`); its
/// messages go in with `chat.send`, as on the desktop.
class ChatBloc extends Bloc<ChatEvent, ChatState> {
  ChatBloc(this._repository, {String? conversationId, this.employeeId, this.projectId, this.session = false})
      : assert(employeeId != null || projectId != null, 'a conversation belongs to an employee or a project'),
        assert(!session || conversationId != null, 'a session has its conversation'),
        group = false,
        channel = null,
        channelTicket = null,
        ticketId = null,
        threadRoot = null,
        runAuthor = null,
        onRead = null,
        liveTimeout = Duration.zero,
        super(ChatState(conversationId: conversationId, loading: conversationId != null)) {
    _init(conversationId != null);
  }

  /// The group chat (Team). Its id comes with the first load. [runAuthor]
  /// names the employee a run answers for while it streams (its answer is no
  /// message in the group until it ends); `null` once that run no longer works.
  /// A live answer that never becomes a message goes [liveTimeout] after its
  /// last update once its run no longer works.
  /// [onRead] runs after each read of the channel (which the server counts as
  /// read): the channels' unread and mentions are asked for again then.
  /// [channelTicket]: the ticket whose channel this is (feature `ticket_channels`).
  ChatBloc.group(this._repository, {this.runAuthor, this.liveTimeout = const Duration(seconds: 3), this.channel, this.channelTicket, this.onRead})
      : employeeId = null,
        projectId = null,
        ticketId = null,
        threadRoot = null,
        session = false,
        group = true,
        super(const ChatState()) {
    _init(true);
  }

  /// A ticket's discussion (feature `ticket_discussion`): read as any
  /// conversation, written with `ticket.post`. It works as the group chat:
  /// board members and employees write in it, answers stream under their name.
  ChatBloc.discussion(this._repository, {required String this.ticketId, required String conversationId, this.runAuthor, this.liveTimeout = const Duration(seconds: 3)})
      : employeeId = null,
        projectId = null,
        channel = null,
        channelTicket = null,
        threadRoot = null,
        onRead = null,
        session = false,
        group = true,
        super(ChatState(conversationId: conversationId)) {
    _init(true);
  }

  /// One thread of a Team chat channel (feature `chat_threads`, AUR-CHAT-002
  /// §5): the message [threadRoot] and its replies (`thread.read`, which marks
  /// the thread read), written with `group_chat.send { channel, thread_parent }`.
  /// It works as the channel: answers stream into it under their name, and
  /// only the ones of this thread.
  ChatBloc.thread(this._repository, {required String this.threadRoot, this.channel, this.channelTicket, this.runAuthor, this.liveTimeout = const Duration(seconds: 3), this.onRead})
      : employeeId = null,
        projectId = null,
        ticketId = null,
        session = false,
        group = true,
        super(const ChatState()) {
    _init(true);
  }

  void _init(bool load) {
    on<ChatRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 80)));
    on<_AppendReceived>(_append, transformer: sequential());
    on<_LiveTicked>(_tick);
    on<ChatMessageSent>(_send, transformer: sequential());
    on<ChatUnsentRetried>(_retryUnsent, transformer: sequential());
    on<ChatUnsentDiscarded>(_discardUnsent);
    on<ChatSecretConfirmed>(_confirmSecret, transformer: sequential());
    on<ChatSecretCheckShown>((e, emit) => emit(state.copyWith()));
    on<ChatQueuedDeleted>(_deleteQueued);
    on<ChatQueuedSentNow>(_sendQueuedNow);
    on<ChatStopRequested>((e, emit) => _run(emit, Commands.stopRun(e.runId)));
    on<ChatReplyRetried>((e, emit) => _whenChat(emit, (id) => _run(emit, Commands.retryReply(id, e.userMessageId))));
    on<ChatModeSelected>(_mode);
    on<ChatBrowserToggled>(_browser);
    on<ChatScreenToggled>(_screen);
    on<ChatRunSelected>(_runSelected);
    on<ChatFileAttached>(_attach, transformer: sequential());
    on<ChatImageAdded>(_addImage, transformer: sequential());
    on<ChatPicked>(_picked, transformer: sequential());
    on<ChatImageRemoved>((e, emit) => emit(state.copyWith(images: state.images.where((i) => i != e.image).toList())));
    on<ChatFileRemoved>((e, emit) => emit(state.copyWith(files: state.files.where((f) => f != e.file).toList())));
    on<ChatAttachmentRemoved>((e, emit) => _run(emit, Commands.removeAttachment(e.attachmentId)));
    on<ChatNoticeShown>((e, emit) => emit(state.copyWith()));
    on<ChatReopened>((e, emit) => emit(state.copyWith(closed: false)));
    on<ChatReacted>(_react, transformer: sequential());
    on<ChatShown>((e, emit) {
      _shown = e.visible;
      if (_shown && _stale) {
        _stale = false;
        add(const ChatRefreshed());
      }
    });

    _subs = [
      _repository.online.listen((_) => _changed()),
      _repository.chatChanges.where((e) => e.aggregateId == state.conversationId || e.isRun).listen((_) => _changed()),
      // An answer streaming into a thread goes to that thread's screen, never into the channel (AUR-CHAT-002).
      _repository.appends.where((a) => a.conversationId == state.conversationId && a.threadParent == threadRoot).listen((a) => add(_AppendReceived(a))),
      // A ticket's channel is its discussion: its lines and employees' posts
      // come as `task.posted` on the conversation, a move as the ticket's own event.
      if (channelTicket case final ticket?)
        _repository.changes.where((e) => (e.kind == EventKind.taskPosted && e.aggregateId == state.conversationId) || e.aggregateId == ticket).listen((_) => _changed()),
    ];
    if (load && _repository.linkState.isOnline) add(const ChatRefreshed());
  }

  final AureliusRepository _repository;
  final String? employeeId;
  final String? projectId;

  /// A work session's conversation: sent with `chat.send`.
  final bool session;

  /// The company's group chat: messages carry their author, answers stream
  /// before they are messages there (see [_live]).
  final bool group;

  /// The group chat's channel (a project's, from `channel.list`); null: General.
  final String? channel;

  /// The ticket whose channel [channel] is; null for General and a project's.
  final String? channelTicket;

  /// The ticket whose discussion this is; null for every other chat.
  final String? ticketId;

  bool get discussion => ticketId != null;

  /// The message whose thread this is; null for every other chat.
  final String? threadRoot;

  /// See [ChatBloc.group].
  final void Function()? onRead;

  /// The Team chat is on screen ([ChatShown]); a change while hidden leaves it [_stale].
  bool _shown = true;
  bool _stale = false;

  /// Reading the Team chat marks its channel read for the board member; a
  /// hidden one waits. Every other chat reads again at once.
  void _changed() {
    if (group && !discussion && !_shown) {
      _stale = true;
    } else {
      add(const ChatRefreshed());
    }
  }

  /// The board members were asked for once (they rarely change).
  bool _membersRead = false;
  final String? Function(String runId)? runAuthor;
  final Duration liveTimeout;
  late final List<StreamSubscription<Object?>> _subs;
  static const _uuid = Uuid();

  /// As many as the PC takes with one message.
  static const maxImages = 8;
  static const maxFiles = 8;
  static const maxFileBytes = 25 * 1024 * 1024;

  /// Waiting messages deleted here and not yet confirmed: kept out of refreshes.
  final Set<String> _deleting = {};

  /// Unsent messages discarded while already on their way: deleted on the PC
  /// as soon as their receipt says which message they became.
  final Set<String> _discarded = {};

  /// Group chat: answers streaming in that are no message there yet, with
  /// their last update. They show at the end until the snapshot has them.
  final Map<String, DateTime> _live = {};

  /// Runs while [_live] has answers: no refresh may come after the last one
  /// ends (an answer that only posted with post_to_group_chat is never copied).
  Timer? _ticker;

  void _watchLive() {
    if (_live.isEmpty) {
      _ticker?.cancel();
      _ticker = null;
    } else {
      _ticker ??= Timer.periodic(liveTimeout < const Duration(seconds: 1) ? liveTimeout : const Duration(seconds: 1), (_) => add(const _LiveTicked()));
    }
  }

  /// Live answers whose run ended without a message go; one without its
  /// author yet gets it once the company knows who runs it.
  void _tick(_LiveTicked event, Emitter<ChatState> emit) {
    final before = Set.of(_live.keys);
    _expireLive(const {});
    _watchLive();
    final gone = before.difference(_live.keys.toSet());
    var named = false;
    final messages = <MessageView>[];
    for (final m in state.messages) {
      if (gone.contains(m.id)) continue;
      final author = _live.containsKey(m.id) && _named(m.author) == null && m.runId != null ? _named(runAuthor?.call(m.runId!)) : null;
      named |= author != null;
      messages.add(author == null ? m : m.withText(m.text, author: author));
    }
    if (gone.isNotEmpty || named) emit(state.copyWith(messages: messages));
  }

  static String? _named(String? author) => author == null || author.trim().isEmpty ? null : author;

  Future<void> _refresh(ChatRefreshed event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    if (id == null && !group) return;
    // Trying again after it failed: loading shows again.
    if (group && state.loadError != null && state.messages.isEmpty) emit(state.copyWith(loading: true));
    if (threadRoot != null) return _refreshThread(threadRoot!, emit);
    try {
      final snapshot = discussion ? await _repository.messages(id!) : group ? await _repository.groupChat(channel: channel) : await _repository.messages(id!);
      final messages = _deleting.isEmpty ? snapshot.messages : snapshot.messages.where((m) => !_deleting.contains(m.id)).toList();
      emit(state.copyWith(
        loading: false,
        conversationId: snapshot.conversationId,
        messages: group ? [...messages, ..._stillLive(messages)] : messages,
        attachments: snapshot.attachments,
        mode: snapshot.agentMode,
        browserAllowed: snapshot.browserAllowed,
        screenAllowed: snapshot.screenAllowed,
        run: (snapshot.run.model, snapshot.run.effort),
        history: snapshot.readOnly ?? false,
        unsent: state.unsent.where((u) => !u.delivered).toList(),
        clearLoadError: true,
        boardMembers: group && !_membersRead ? await _boardMembers() : null,
      ));
      if (group && !discussion) onRead?.call();
    } on DisconnectedException catch (e) {
      // The Team chat that never loaded says why (a reconnect loads it again), not that it is empty.
      emit(state.copyWith(loading: false, loadError: group && state.conversationId == null ? e.message : null));
    } on ProtocolException catch (e) {
      final shown = discussion ? 'The discussion could not be loaded: ${e.shown}' : group ? groupChatFailure(e) : e.shown;
      emit(state.copyWith(loading: false, notice: shown, loadError: shown));
    } catch (e) {
      // Anything else (an answer this app cannot read) stops the loading too.
      if (!group) rethrow;
      emit(state.copyWith(loading: false, loadError: '${discussion ? 'The discussion' : 'The Team chat'} could not be loaded: $e'));
    }
  }

  /// A thread's read: its root on top, then its replies. The PC says whether
  /// its channel is closed (`read_only`): then nothing in it can be written.
  Future<void> _refreshThread(String root, Emitter<ChatState> emit) async {
    if (state.loadError != null && state.messages.isEmpty) emit(state.copyWith(loading: true));
    try {
      final thread = await _repository.thread(root);
      final messages = [thread.root, ...thread.replies];
      emit(state.copyWith(
        loading: false,
        conversationId: thread.conversationId,
        messages: [...messages, ..._stillLive(messages)],
        unsent: state.unsent.where((u) => !u.delivered).toList(),
        closed: thread.readOnly ?? false,
        clearLoadError: true,
        boardMembers: !_membersRead ? await _boardMembers() : null,
      ));
      onRead?.call();
    } on DisconnectedException catch (e) {
      emit(state.copyWith(loading: false, loadError: state.conversationId == null ? e.message : null));
    } on ProtocolException catch (e) {
      final shown = 'The thread could not be loaded: ${e.shown}';
      emit(state.copyWith(loading: false, notice: shown, loadError: shown));
    } catch (e) {
      emit(state.copyWith(loading: false, loadError: 'The thread could not be loaded: $e'));
    }
  }

  /// The board members' names (`user.list`); none when they cannot be read.
  Future<List<String>> _boardMembers() async {
    _membersRead = true;
    try {
      return [for (final u in await _repository.boardMembers()) u.username];
    } on Object {
      return const [];
    }
  }

  /// Why the Team chat did not load. A server from before the group chat does not
  /// know `group_chat.get` (`unknown_method`): after an update it still runs the
  /// old version and must be restarted.
  static String groupChatFailure(ProtocolException e) => e.code == ErrorCode.unknownMethod
      ? 'The server is older than this app and has no Team chat yet. Restart the server (or update it), then try again.'
      : 'The Team chat could not be loaded: ${e.shown}';

  /// The streaming group answers the snapshot does not have yet. One that
  /// never became a message (an empty answer) goes once its run no longer
  /// works and no update came for a moment, as on the desktop.
  List<MessageView> _stillLive(List<MessageView> snapshot) {
    _expireLive({for (final m in snapshot) m.id});
    _watchLive();
    return [for (final m in state.messages) if (_live.containsKey(m.id)) m];
  }

  /// Drops the live answers that are messages now ([arrived]) or whose run
  /// no longer works and that had no update for [liveTimeout].
  void _expireLive(Set<String> arrived) {
    final now = DateTime.now();
    _live.removeWhere((id, at) {
      if (arrived.contains(id)) return true;
      final m = state.messages.where((m) => m.id == id).firstOrNull;
      if (m == null) return true;
      final running = m.runId != null && runAuthor?.call(m.runId!) != null;
      return !running && now.difference(at) > liveTimeout;
    });
  }

  void _append(_AppendReceived event, Emitter<ChatState> emit) {
    final append = event.append;
    final index = state.messages.indexWhere((m) => m.id == append.messageId);
    if (index < 0) {
      // Group chat: an answer that is no message there until it ends.
      if (group) return _startLive(append, emit);
      // A message we have not seen yet (just started): fetch it.
      add(const ChatRefreshed());
      return;
    }
    final next = append.applyTo(state.messages[index].text);
    if (next == null) {
      // A live group answer cannot be fetched; the next whole update repairs it.
      if (!_live.containsKey(append.messageId)) add(const ChatRefreshed());
      return;
    }
    if (_live.containsKey(append.messageId)) _live[append.messageId] = DateTime.now();
    final messages = [...state.messages];
    final author = _named(messages[index].author) ?? (_live.containsKey(append.messageId) ? runAuthor?.call(append.runId) : null);
    messages[index] = messages[index].withText(next, activity: append.activity, author: author);
    final progress = event.append.progress;
    emit(state.copyWith(messages: messages, progress: progress == null ? null : {...state.progress, event.append.messageId: progress}));
  }

  /// The first update of a streaming group answer (each session's first one
  /// carries the whole text so far; a later piece alone is not shown).
  void _startLive(StreamAppend append, Emitter<ChatState> emit) {
    final text = append.applyTo('');
    if (text == null) return;
    _live[append.messageId] = DateTime.now();
    _watchLive();
    final message = MessageView(
      activity: append.activity ?? const [],
      author: runAuthor?.call(append.runId),
      createdAtMs: DateTime.now().millisecondsSinceEpoch,
      files: const [],
      id: append.messageId,
      // The mode the employee answers in, shown with its name.
      mode: append.mode,
      ordinal: state.messages.isEmpty ? 0 : state.messages.last.ordinal + 1,
      queued: false,
      role: MessageRole.assistant,
      runId: append.runId,
      sources: const [],
      status: MessageStatus.partial,
      text: text,
      uploads: const [],
    );
    final progress = append.progress;
    emit(state.copyWith(messages: [...state.messages, message], progress: progress == null ? null : {...state.progress, append.messageId: progress}));
  }

  Future<void> _send(ChatMessageSent event, Emitter<ChatState> emit) async {
    final text = event.text.trim();
    if (text.isEmpty && state.images.isEmpty && state.files.isEmpty) return;
    final unsent = UnsentMessage(localId: _uuid.v4(), idempotencyKey: _uuid.v7(), text: text, images: state.images, files: state.files, interrupt: event.now);
    emit(state.copyWith(unsent: [...state.unsent, unsent], images: const [], files: const []));
    // Stop first, so the running answer never answers this message; the new
    // answer starts once it has stopped and sees what it had written. The PC
    // also stops it with the message itself (`interrupt`), even when this view
    // doesn't know the run yet; the separate stop is for PCs from before that.
    // The group chat has no "Send now": a message there wakes whom it names.
    final running = group ? null : state.streaming?.runId;
    if (event.now && running != null) {
      try {
        await _repository.write(Commands.stopRun(running), retry: false);
      } on NotSentException catch (e) {
        emit(state.copyWith(notice: e.toString()));
      } on ProtocolException catch (e) {
        emit(state.copyWith(notice: e.message));
      }
    }
    await _deliver(unsent, emit);
  }

  Future<void> _retryUnsent(ChatUnsentRetried event, Emitter<ChatState> emit) async {
    final unsent = state.unsent.where((u) => u.localId == event.localId).firstOrNull;
    if (unsent == null) return;
    emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asFailed(false) : u).toList()));
    await _deliver(unsent.asFailed(false), emit);
  }

  Future<void> _deliver(UnsentMessage unsent, Emitter<ChatState> emit) async {
    // Discarded before it went out (while "Send now" stopped the answer).
    if (!state.unsent.any((u) => u.localId == unsent.localId)) {
      _discarded.remove(unsent.idempotencyKey);
      return;
    }
    final images = [for (final i in unsent.images) i.path];
    final files = [for (final f in unsent.files) f.path];
    final fresh = !group && employeeId == null && state.conversationId == null;
    final command = discussion
        ? Commands.postTicketMessage(ticketId!, unsent.text, images: images, files: files, confirmSecret: unsent.confirmSecret, threadParent: threadRoot)
        : group
        ? Commands.sendGroupMessage(unsent.text, images: images, files: files, channel: channel, confirmSecret: unsent.confirmSecret, threadParent: threadRoot)
        : session
        ? Commands.sendChatMessage(state.conversationId!, unsent.text, images: images, files: files, interrupt: unsent.interrupt, confirmSecret: unsent.confirmSecret)
        : employeeId != null
        ? Commands.sendEmployeeMessage(employeeId!, unsent.text, conversationId: state.conversationId!, images: images, files: files, interrupt: unsent.interrupt, confirmSecret: unsent.confirmSecret)
        : Commands.sendProjectMessage(
            projectId!,
            unsent.text,
            conversationId: state.conversationId,
            images: images,
            files: files,
            interrupt: unsent.interrupt,
            mode: fresh && state.mode != AgentMode.ask ? state.mode : null,
            browserAllowed: fresh && state.browserAllowed,
            screenAllowed: fresh && state.screenAllowed,
            confirmSecret: unsent.confirmSecret,
          );
    try {
      final receipt = await _repository.write(command, idempotencyKey: unsent.idempotencyKey);
      final (sentTo, messageId) = _sentMessage(receipt);
      final conversationId = sentTo ?? state.conversationId;
      if (_discarded.remove(unsent.idempotencyKey)) {
        emit(state.copyWith(conversationId: conversationId));
        await _takeBack(conversationId, messageId, emit);
        return;
      }
      emit(state.copyWith(
        conversationId: conversationId,
        unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asDelivered() : u).toList(),
      ));
      add(const ChatRefreshed());
    } on NotSentException {
      emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asFailed(true) : u).toList()));
    } on ProtocolException catch (e) {
      // Sent anyway and refused again (a value in an attached file): asking
      // again would only loop (M-m5). It stays unsent, to change or discard.
      if (e.looksLikeSecret && unsent.confirmSecret) {
        emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asFailed(true) : u).toList(), notice: secretCheckNoSendAnyway));
        return;
      }
      if (e.looksLikeSecret) {
        // B10: nothing was stored; the board member chooses (only the names are shown).
        emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asSecret(e.matchedItems) : u).toList(), secretCheck: unsent.localId));
        return;
      }
      // A closed channel (its ticket on Done or archived): the read-only bar
      // says so, never the error itself (AUR-CHAT-001 §8). The text stays to copy.
      if (e.code == ErrorCode.channelArchived) {
        emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asFailed(true) : u).toList(), closed: true));
        onRead?.call();
        return;
      }
      // A server without the discussion engine (`unknown_method`): kept, to try again after an update.
      emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? u.asFailed(true) : u).toList(), notice: discussion ? discussionPostFailure(e) : e.message));
    }
  }

  /// The reaction shows at once; refused, it goes back. A closed channel
  /// (`channel_archived`) turns read-only, as a refused send does.
  Future<void> _react(ChatReacted event, Emitter<ChatState> emit) async {
    final before = state.messages.where((m) => m.id == event.messageId).firstOrNull;
    if (before == null) return;
    final me = _repository.linkState.user?.username;
    void put(List<ReactionView> reactions, {String? notice, bool? closed}) => emit(state.copyWith(
          messages: [for (final m in state.messages) m.id == event.messageId ? m.withReactions(reactions) : m],
          notice: notice,
          closed: closed,
        ));
    put(reactedLocally(before.reactions, event.emoji, on: event.on, me: me));
    try {
      await _repository.write(Commands.react(event.messageId, event.emoji, on: event.on), retry: false);
    } on NotSentException catch (e) {
      put(before.reactions, notice: e.toString());
    } on ProtocolException catch (e) {
      if (e.code == ErrorCode.channelArchived) {
        put(before.reactions, closed: true);
        onRead?.call();
        return;
      }
      put(before.reactions, notice: e.message);
    }
  }

  Future<void> _confirmSecret(ChatSecretConfirmed event, Emitter<ChatState> emit) async {
    final unsent = state.unsent.where((u) => u.localId == event.localId).firstOrNull;
    if (unsent == null) return;
    final confirmed = unsent.asConfirmed(_uuid.v7());
    emit(state.copyWith(unsent: state.unsent.map((u) => u.localId == unsent.localId ? confirmed : u).toList()));
    await _deliver(confirmed, emit);
  }

  Future<void> _discardUnsent(ChatUnsentDiscarded event, Emitter<ChatState> emit) async {
    final unsent = state.unsent.where((u) => u.localId == event.localId).firstOrNull;
    if (unsent == null) return;
    emit(state.copyWith(unsent: state.unsent.where((u) => u.localId != event.localId).toList()));
    // Failed: nothing went out. Still on this phone: taken back. Otherwise
    // the PC may have it already: removed there once its receipt arrives.
    if (unsent.failed || _repository.withdraw(unsent.idempotencyKey)) return;
    _discarded.add(unsent.idempotencyKey);
  }

  /// A message discarded here that reached the PC anyway: delete it there
  /// while it still waits.
  Future<void> _takeBack(String? conversationId, String? messageId, Emitter<ChatState> emit) async {
    const already = 'It was sent already; delete it from the conversation if it still waits.';
    if (conversationId == null || messageId == null) {
      add(const ChatRefreshed());
      emit(state.copyWith(notice: already));
      return;
    }
    try {
      await _repository.write(Commands.deleteQueuedMessage(conversationId, messageId), retry: false);
    } on ProtocolException {
      // Its answer started already.
      emit(state.copyWith(notice: 'It was sent already and is being answered.'));
    } on NotSentException {
      emit(state.copyWith(notice: already));
    }
    add(const ChatRefreshed());
  }

  /// Why a waiting message can't be changed from here, if so.
  String? _cannotChangeQueued() {
    final link = _repository.linkState;
    if (!link.isOnline) return 'Not connected to the PC.';
    return null;
  }

  Future<void> _deleteQueued(ChatQueuedDeleted event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    final index = state.messages.indexWhere((m) => m.id == event.messageId);
    if (id == null || index < 0) return;
    final refused = _cannotChangeQueued();
    if (refused != null) return emit(state.copyWith(notice: refused));
    final removed = state.messages[index];
    _deleting.add(removed.id);
    emit(state.copyWith(messages: [...state.messages]..removeAt(index)));
    try {
      await _repository.write(Commands.deleteQueuedMessage(id, removed.id), retry: false);
      _deleting.remove(removed.id);
      add(const ChatRefreshed());
    } catch (e) {
      _deleting.remove(removed.id);
      final messages = [...state.messages]..insert(index.clamp(0, state.messages.length), removed);
      emit(state.copyWith(messages: messages, notice: e is ProtocolException ? e.message : e.toString()));
      add(const ChatRefreshed());
    }
  }

  Future<void> _sendQueuedNow(ChatQueuedSentNow event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    if (id == null || !state.messages.any((m) => m.id == event.messageId)) return;
    final refused = _cannotChangeQueued();
    if (refused != null) return emit(state.copyWith(notice: refused));
    await _run(emit, Commands.sendQueuedNow(id, event.messageId));
  }

  /// The conversation and message a send landed in.
  (String?, String?) _sentMessage(Receipt receipt) => switch (receipt.command) {
        CommandResultEmployeeMessageSent(:final conversationId, :final messageId) => (conversationId, messageId),
        CommandResultProjectMessageSent(:final conversationId, :final messageId) => (conversationId, messageId),
        CommandResultGroupMessageSent(:final conversationId, :final messageId) => (conversationId, messageId),
        CommandResultTicketMessagePosted(:final conversationId, :final messageId) => (conversationId, messageId),
        _ => (null, null),
      };

  Future<void> _whenChat(Emitter<ChatState> emit, Future<void> Function(String conversationId) action) async {
    final id = state.conversationId;
    if (id == null) return;
    await action(id);
  }

  Future<void> _run(Emitter<ChatState> emit, Map<String, dynamic> command) async {
    try {
      await _repository.write(command, retry: false);
      add(const ChatRefreshed());
    } on NotSentException catch (e) {
      emit(state.copyWith(notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(notice: e.message));
    }
  }

  Future<void> _mode(ChatModeSelected event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    if (id == null) {
      // A new project conversation: kept here and sent with the first message.
      if (employeeId == null) return emit(state.copyWith(mode: event.mode));
      emit(state.copyWith(notice: 'Send a first message, then pick a mode.'));
      return;
    }
    final before = state.mode;
    emit(state.copyWith(mode: event.mode));
    try {
      await _repository.write(Commands.setAgentMode(id, event.mode), retry: false);
      _rememberMode(event.mode);
    } catch (e) {
      emit(state.copyWith(mode: before, notice: e.toString()));
    }
  }

  /// The employee's next task or conversation starts in this mode (see [ModeMemory]).
  void _rememberMode(AgentMode mode) {
    final employee = employeeId;
    if (employee == null) return;
    try {
      unawaited(ModeMemory.write(_repository.host.hostId, employee, mode.wire));
    } on Object {
      // No host (tests): not remembered.
    }
  }

  Future<void> _screen(ChatScreenToggled event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    if (id == null) {
      if (employeeId == null) return emit(state.copyWith(screenAllowed: !state.screenAllowed));
      emit(state.copyWith(notice: 'Send a first message, then allow the screen.'));
      return;
    }
    final next = !state.screenAllowed;
    emit(state.copyWith(screenAllowed: next));
    try {
      await _repository.write(Commands.setScreenAllowed(id, next), retry: false);
    } on ProtocolException catch (e) {
      // Turning it on needs the Approve right; the PC says no otherwise.
      emit(state.copyWith(screenAllowed: !next, notice: e.message));
    } catch (e) {
      emit(state.copyWith(screenAllowed: !next, notice: e.toString()));
    }
  }

  Future<void> _runSelected(ChatRunSelected event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    if (id == null) {
      emit(state.copyWith(notice: 'Send a first message, then pick a model.'));
      return;
    }
    final before = (state.runModel, state.runEffort);
    emit(state.copyWith(run: (event.model, event.effort)));
    try {
      await _repository.write(Commands.setConversationRun(id, model: event.model, effort: event.effort), retry: false);
    } on ProtocolException catch (e) {
      emit(state.copyWith(run: before, notice: e.message));
    } catch (e) {
      emit(state.copyWith(run: before, notice: e.toString()));
    }
  }

  Future<void> _browser(ChatBrowserToggled event, Emitter<ChatState> emit) async {
    final id = state.conversationId;
    if (id == null) {
      if (employeeId == null) return emit(state.copyWith(browserAllowed: !state.browserAllowed));
      emit(state.copyWith(notice: 'Send a first message, then allow the browser.'));
      return;
    }
    final next = !state.browserAllowed;
    emit(state.copyWith(browserAllowed: next));
    try {
      await _repository.write(Commands.setBrowserAllowed(id, next), retry: false);
    } on ProtocolException catch (e) {
      // Turning it on needs the Approve right; the PC says no otherwise.
      emit(state.copyWith(browserAllowed: !next, notice: e.message));
    } catch (e) {
      emit(state.copyWith(browserAllowed: !next, notice: e.toString()));
    }
  }

  /// A file: uploaded now, sent with the next message (the PC copies it into
  /// the work folder).
  Future<void> _attach(ChatFileAttached event, Emitter<ChatState> emit) async {
    if (!_repository.linkState.isOnline) {
      emit(state.copyWith(notice: 'Not connected to the PC.'));
      return;
    }
    if (state.files.length >= maxFiles) {
      emit(state.copyWith(notice: 'At most $maxFiles files per message.'));
      return;
    }
    emit(state.copyWith(uploading: true));
    try {
      if (await event.file.length() > maxFileBytes) {
        emit(state.copyWith(uploading: false, notice: '${event.name} is larger than 25 MB.'));
        return;
      }
      final uploaded = await _repository.upload(event.file, event.name);
      emit(state.copyWith(uploading: false, files: [...state.files, PendingFile(path: uploaded.path, name: uploaded.name, byteLength: uploaded.byteLength)]));
    } catch (e) {
      emit(state.copyWith(uploading: false, notice: e.toString()));
    }
  }

  Future<void> _picked(ChatPicked event, Emitter<ChatState> emit) async {
    final tooLarge = <String>[];
    final fits = <(File, String)>[];
    for (final (file, name) in event.picked) {
      if (await file.length() > maxFileBytes) {
        tooLarge.add(name);
      } else {
        fits.add((file, name));
      }
    }
    final room = event.images ? maxImages - state.images.length : maxFiles - state.files.length;
    final kept = fits.take(room.clamp(0, fits.length)).toList();
    final notice = [
      if (tooLarge.isNotEmpty) '${_names(tooLarge)} ${tooLarge.length == 1 ? 'is' : 'are'} larger than 25 MB and ${tooLarge.length == 1 ? 'was' : 'were'} not added.',
      if (kept.length < fits.length) 'At most ${event.images ? '$maxImages images' : '$maxFiles files'} per message; ${fits.length - kept.length} not added.',
    ].join(' ');
    if (notice.isNotEmpty) emit(state.copyWith(notice: notice));
    for (final (file, name) in kept) {
      if (event.images) {
        await _addImage(ChatImageAdded(file, name), emit);
      } else {
        await _attach(ChatFileAttached(file, name), emit);
      }
    }
  }

  static String _names(List<String> names) => names.length == 1 ? names.single : '${names.take(names.length - 1).join(', ')} and ${names.last}';

  Future<void> _addImage(ChatImageAdded event, Emitter<ChatState> emit) async {
    if (state.images.length >= maxImages) {
      emit(state.copyWith(notice: 'At most $maxImages images per message.'));
      return;
    }
    emit(state.copyWith(uploading: true));
    try {
      final uploaded = await _repository.upload(event.file, event.name);
      emit(state.copyWith(uploading: false, images: [...state.images, PendingImage(local: event.file, path: uploaded.path, name: event.name)]));
    } catch (e) {
      emit(state.copyWith(uploading: false, notice: e.toString()));
    }
  }

  @override
  Future<void> close() async {
    _ticker?.cancel();
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
