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

class ChannelsState extends Equatable {
  const ChannelsState({this.supported = false, this.tickets = false, this.channels = const [], this.error});

  /// The server has channels (feature `team_channels`); else the Team chat is General alone.
  final bool supported;

  /// The server has a channel per ticket (feature `ticket_channels`).
  final bool tickets;

  /// General first, then one per project (`channel.list`).
  final List<ChannelView> channels;

  /// The last read failed (the channels shown are the ones read before).
  final String? error;

  ChannelView? byId(String? conversationId) => channels.where((c) => c.conversationId == conversationId).firstOrNull;

  ChannelView? get general => channels.where((c) => c.projectId == null).firstOrNull;

  static List<Object?> _rows(List<ChannelView>? channels) => [
        for (final c in channels ?? const <ChannelView>[])
          (c.conversationId, c.name, c.unread, c.mentions, c.archived, c.lastActivityMs, c.kind, c.ticketColumn, c.ticketNumber, c.readThrough),
      ];

  @override
  List<Object?> get props => [supported, tickets, _rows(channels), error];
}

/// The Team chat's channels (MU2-08), for a link that opens one (`/channel/:cid`,
/// ticket #71: there is no channel list any more); read again when the link
/// comes back and when a chat changes.
class ChannelsBloc extends Bloc<ChannelsEvent, ChannelsState> {
  ChannelsBloc(this._repository) : super(const ChannelsState()) {
    on<ChannelsRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 300)));
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
      emit(ChannelsState(supported: true, tickets: tickets, channels: channels));
    } on DisconnectedException {
      // Read again once online.
    } on ProtocolException catch (e) {
      emit(ChannelsState(supported: true, tickets: tickets, channels: state.channels, error: e.shown));
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
