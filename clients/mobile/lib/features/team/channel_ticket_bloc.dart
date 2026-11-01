import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart';
import '../../data/views.dart';

sealed class ChannelTicketEvent {
  const ChannelTicketEvent();
}

class ChannelTicketRefreshed extends ChannelTicketEvent {
  const ChannelTicketRefreshed();
}

/// Whether the channel's header is on screen: hidden, a change only marks it
/// stale and it is read again once it shows.
class ChannelTicketShown extends ChannelTicketEvent {
  const ChannelTicketShown(this.visible);

  final bool visible;
}

class ChannelTicketState extends Equatable {
  const ChannelTicketState({this.loading = true, this.ticket, this.assignees = const [], this.error});

  final bool loading;

  /// The ticket of the channel (`ticket.get`); null until read.
  final TicketView? ticket;

  /// The owner first, then the other employees of its tasks.
  final List<String> assignees;

  /// The last read failed: why, in words (the header keeps what it had).
  final String? error;

  @override
  List<Object?> get props => [loading, ticket?.id, ticket?.version, ticket?.column, ticket?.title, assignees, error];
}

/// The ticket behind a ticket's channel, for its header (AUR-CHAT-001 §3, §8
/// O-1: title and assignees come from `ticket.get` when the channel opens);
/// read again when the board changes while it shows.
class ChannelTicketBloc extends Bloc<ChannelTicketEvent, ChannelTicketState> {
  ChannelTicketBloc(this._repository, this.ticketId) : super(const ChannelTicketState()) {
    on<ChannelTicketRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 300)));
    on<ChannelTicketShown>((e, emit) {
      _shown = e.visible;
      if (_shown && _stale) {
        _stale = false;
        add(const ChannelTicketRefreshed());
      }
    });
    _subs = [
      _repository.online.listen((_) => _changed()),
      _repository.boardChanges.listen((_) => _changed()),
    ];
    if (_repository.linkState.isOnline) add(const ChannelTicketRefreshed());
  }

  final AureliusRepository _repository;
  final String ticketId;
  late final List<StreamSubscription<Object?>> _subs;
  bool _shown = true;
  bool _stale = false;

  void _changed() {
    if (_shown) {
      add(const ChannelTicketRefreshed());
    } else {
      _stale = true;
    }
  }

  Future<void> _refresh(ChannelTicketRefreshed event, Emitter<ChannelTicketState> emit) async {
    try {
      final detail = await _repository.ticket(ticketId);
      emit(ChannelTicketState(loading: false, ticket: detail.ticket, assignees: ticketAssignees(detail)));
    } on DisconnectedException {
      emit(ChannelTicketState(loading: false, ticket: state.ticket, assignees: state.assignees));
    } on ProtocolException catch (e) {
      emit(ChannelTicketState(loading: false, ticket: state.ticket, assignees: state.assignees, error: e.shown));
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
