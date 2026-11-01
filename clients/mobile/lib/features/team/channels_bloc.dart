import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart';
import '../../data/discussion.dart';
import '../../data/views.dart';

sealed class ChannelsEvent {
  const ChannelsEvent();
}

class ChannelsRefreshed extends ChannelsEvent {
  const ChannelsRefreshed();
}

/// The search field of the channel list (`channel.search`); empty: no search.
class ChannelsSearched extends ChannelsEvent {
  const ChannelsSearched(this.query);

  final String query;
}

class ChannelsState extends Equatable {
  const ChannelsState({this.supported = false, this.tickets = false, this.channels = const [], this.error, this.query = '', this.found, this.searchError});

  /// The server has channels (feature `team_channels`); else the Team chat is General alone.
  final bool supported;

  /// The server has a channel per ticket (feature `ticket_channels`): the
  /// Team tab opens on the channel list (AUR-CHAT-001 §6).
  final bool tickets;

  /// What is searched for; empty: the whole list shows.
  final String query;

  /// What `channel.search` found for [query]; null while it is asked for.
  final List<ChannelView>? found;

  /// The search failed: why, in words.
  final String? searchError;

  /// General first, then one per project (`channel.list`).
  final List<ChannelView> channels;

  /// The last read failed (the channels shown are the ones read before).
  final String? error;

  ChannelView? byId(String? conversationId) => channels.where((c) => c.conversationId == conversationId).firstOrNull;

  ChannelView? get general => channels.where((c) => c.projectId == null).firstOrNull;

  /// The Team tab's badge (B8): the mentions, and a dot for anything unread.
  ({int mentions, bool unread}) get badge => teamBadge(channels);

  ChannelsState copyWith({String? query, List<ChannelView>? found, bool clearFound = false, String? searchError}) => ChannelsState(
        supported: supported,
        tickets: tickets,
        channels: channels,
        error: error,
        query: query ?? this.query,
        found: clearFound ? null : found ?? this.found,
        searchError: searchError,
      );

  static List<Object?> _rows(List<ChannelView>? channels) => [
        for (final c in channels ?? const <ChannelView>[])
          (c.conversationId, c.name, c.unread, c.mentions, c.archived, c.lastActivityMs, c.kind, c.ticketColumn, c.ticketNumber, c.readThrough),
      ];

  @override
  List<Object?> get props => [supported, tickets, _rows(channels), error, query, found == null, _rows(found), searchError];
}

/// The Team chat's channels (MU2-08) for the picker and the tab's badge; read
/// again when the link comes back and when a chat changes.
class ChannelsBloc extends Bloc<ChannelsEvent, ChannelsState> {
  ChannelsBloc(this._repository) : super(const ChannelsState()) {
    on<ChannelsRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 300)));
    on<ChannelsSearched>(_search, transformer: debounceRestartable(const Duration(milliseconds: 250)));
    _subs = [
      _repository.online.listen((_) => add(const ChannelsRefreshed())),
      _repository.chatChanges.listen((_) => add(const ChannelsRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const ChannelsRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<Object?>> _subs;

  Future<void> _refresh(ChannelsRefreshed event, Emitter<ChannelsState> emit) async {
    if (!_repository.linkState.has(teamChannelsFeature)) {
      emit(const ChannelsState());
      return;
    }
    final tickets = _repository.linkState.has(ticketChannelsFeature);
    // Without the feature a ticket's channel is no channel here (today's list).
    List<ChannelView> shown(List<ChannelView> all) => tickets ? all : [for (final c in all) if (!isTicketChannel(c)) c];
    try {
      final channels = shown(await _repository.channels());
      emit(ChannelsState(supported: true, tickets: tickets, channels: channels, query: state.query, found: state.found));
      if (tickets && state.query.isNotEmpty) add(ChannelsSearched(state.query));
    } on DisconnectedException {
      // Read again once online.
    } on ProtocolException catch (e) {
      emit(ChannelsState(supported: true, tickets: tickets, channels: state.channels, error: e.shown, query: state.query, found: state.found));
    }
  }

  Future<void> _search(ChannelsSearched event, Emitter<ChannelsState> emit) async {
    final query = event.query.trim();
    if (query.isEmpty) return emit(state.copyWith(query: '', clearFound: true));
    emit(state.copyWith(query: query, found: query == state.query ? state.found : null, clearFound: query != state.query));
    try {
      emit(state.copyWith(found: await _repository.searchChannels(query)));
    } on DisconnectedException catch (e) {
      emit(state.copyWith(found: const [], searchError: e.message));
    } on ProtocolException catch (e) {
      emit(state.copyWith(found: const [], searchError: e.shown));
    }
  }

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
