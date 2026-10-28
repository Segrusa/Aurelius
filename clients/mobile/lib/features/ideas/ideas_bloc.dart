import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class IdeasEvent {
  const IdeasEvent();
}

class IdeasRefreshed extends IdeasEvent {
  const IdeasRefreshed();
}

class IdeasFilterChanged extends IdeasEvent {
  const IdeasFilterChanged(this.status);

  final IdeaStatus status;
}

class IdeasProjectChanged extends IdeasEvent {
  const IdeasProjectChanged(this.projectId);

  /// Null shows every project.
  final String? projectId;
}

class IdeasKindChanged extends IdeasEvent {
  const IdeasKindChanged(this.kind);

  final IdeaKind? kind;
}

class IdeasCommandSent extends IdeasEvent {
  const IdeasCommandSent(this.command, {this.done});

  final Map<String, dynamic> command;
  final String? done;
}

class IdeasNoticeShown extends IdeasEvent {
  const IdeasNoticeShown();
}

class IdeasState extends Equatable {
  const IdeasState({this.loading = true, this.ideas = const [], this.filter = IdeaStatus.open, this.kind, this.project, this.notice});

  final bool loading;
  final List<IdeaView> ideas;
  final IdeaStatus filter;

  /// Null shows both kinds.
  final IdeaKind? kind;

  /// The company-wide list narrowed to one project; null shows every project.
  final String? project;
  final String? notice;

  Iterable<IdeaView> get _inProject => ideas.where((i) => project == null || i.projectId == project);
  List<IdeaView> get visible => _inProject.where((i) => i.status == filter && (kind == null || i.kind == kind)).toList();
  int get open => _inProject.where((i) => i.status == IdeaStatus.open).length;

  IdeasState copyWith({bool? loading, List<IdeaView>? ideas, IdeaStatus? filter, IdeaKind? kind, bool clearKind = false, String? project, bool clearProject = false, String? notice}) => IdeasState(
        loading: loading ?? this.loading,
        ideas: ideas ?? this.ideas,
        filter: filter ?? this.filter,
        kind: clearKind ? null : kind ?? this.kind,
        project: clearProject ? null : project ?? this.project,
        notice: notice,
      );

  @override
  List<Object?> get props => [loading, ideas, filter, kind, project, notice];
}

/// A project's ideas, or the ideas of every project when [projectId] is null, kept fresh while shown.
class IdeasBloc extends Bloc<IdeasEvent, IdeasState> {
  IdeasBloc(this._repository, [this.projectId]) : super(const IdeasState()) {
    on<IdeasRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 250)));
    on<IdeasFilterChanged>((e, emit) => emit(state.copyWith(filter: e.status)));
    on<IdeasProjectChanged>((e, emit) => emit(state.copyWith(project: e.projectId, clearProject: e.projectId == null)));
    on<IdeasKindChanged>((e, emit) => emit(state.copyWith(kind: e.kind, clearKind: e.kind == null)));
    on<IdeasCommandSent>(_command);
    on<IdeasNoticeShown>((e, emit) => emit(state.copyWith()));
    _subs = [
      _repository.online.listen((_) => add(const IdeasRefreshed())),
      _repository.companyChanges.where((e) => e.isIdea || e.isTask).listen((_) => add(const IdeasRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const IdeasRefreshed());
  }

  final AureliusRepository _repository;

  /// The project whose ideas this shows; null for the company-wide list, where each idea has its own.
  final String? projectId;

  /// Auto needs the Approve right on this phone; the server refuses it otherwise.
  bool get canStartInAuto => _repository.linkState.can('approve');

  /// Whether a started idea may be tried in several ways; null hides the picker.
  Future<VariantsSetting?> variantsSetting() => _repository.variantsSetting();

  /// The server keeps files with ideas and can delete an idea (`idea.delete`);
  /// older servers have neither, so their UI is hidden.
  bool get attachmentsEnabled => _repository.linkState.has('idea_attachments');
  late final List<StreamSubscription<void>> _subs;

  Future<void> _refresh(IdeasRefreshed event, Emitter<IdeasState> emit) async {
    try {
      emit(state.copyWith(loading: false, ideas: projectId == null ? await _repository.allIdeas() : await _repository.ideas(projectId!)));
    } on DisconnectedException {
      emit(state.copyWith(loading: false));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, notice: e.message));
    }
  }

  Future<void> _command(IdeasCommandSent event, Emitter<IdeasState> emit) async {
    try {
      await _repository.write(event.command, retry: false);
      emit(state.copyWith(notice: event.done));
      add(const IdeasRefreshed());
    } on NotSentException catch (e) {
      emit(state.copyWith(notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(notice: e.message));
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
