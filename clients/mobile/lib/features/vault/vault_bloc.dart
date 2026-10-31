import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class VaultEvent {
  const VaultEvent();
}

class VaultRefreshed extends VaultEvent {
  const VaultRefreshed();
}

/// Show another scope: a project, or the company (null).
class VaultScopeChanged extends VaultEvent {
  const VaultScopeChanged(this.projectId);

  final String? projectId;
}

class VaultNoticeShown extends VaultEvent {
  const VaultNoticeShown();
}

class VaultState extends Equatable {
  const VaultState({this.projectId, this.loading = true, this.list, this.error, this.notice});

  /// The scope shown; null: the company.
  final String? projectId;
  final bool loading;

  /// What `vault.list` answered for [projectId]; null before the first answer.
  final VaultList? list;

  /// Why the list could not be read (shown in place of it).
  final String? error;
  final String? notice;

  List<VaultItemView> get items => list?.items ?? const [];

  /// For a project: the company's items its runs get too (MU4-02 AC1).
  List<VaultItemView> get companyItems => list?.companyItems ?? const [];

  /// The items of a scope by its count, for the scope sheet.
  int countOf(String? projectId) => list?.counts.where((c) => c.projectId == projectId).firstOrNull?.count ?? 0;

  /// One item of this scope or, in a project, of the company.
  VaultItemView? item(String id) => [...items, ...companyItems].where((i) => i.id == id).firstOrNull;

  VaultState copyWith({String? projectId, bool clearProject = false, bool? loading, VaultList? list, bool clearList = false, String? error, String? notice}) => VaultState(
        projectId: clearProject ? null : projectId ?? this.projectId,
        loading: loading ?? this.loading,
        list: clearList ? null : list ?? this.list,
        error: error,
        notice: notice,
      );

  @override
  List<Object?> get props => [projectId, loading, list, error, notice];
}

/// The vault of one scope (Fase 4), kept fresh while shown: the company's
/// items, or a project's with the company's apart. Never holds a secret's
/// value; a shown one lives in [RevealBloc] only while it shows.
class VaultBloc extends Bloc<VaultEvent, VaultState> {
  VaultBloc(this._repository, {String? projectId}) : super(VaultState(projectId: projectId)) {
    on<VaultRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 150)));
    on<VaultScopeChanged>((e, emit) {
      if (e.projectId == state.projectId) return;
      emit(VaultState(projectId: e.projectId));
      add(const VaultRefreshed());
    });
    on<VaultNoticeShown>((e, emit) => emit(state.copyWith()));
    _subs = [
      _repository.online.listen((_) => add(const VaultRefreshed())),
      _repository.vaultChanges.listen((_) => add(const VaultRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const VaultRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<void>> _subs;

  /// This phone's board member (the audit log says "You" for it).
  String? get me => _repository.linkState.user?.id;

  Future<void> _refresh(VaultRefreshed event, Emitter<VaultState> emit) async {
    final scope = state.projectId;
    try {
      final list = await _repository.vault(projectId: scope);
      if (state.projectId != scope) return;
      emit(state.copyWith(loading: false, list: list));
    } on DisconnectedException {
      emit(state.copyWith(loading: false, error: state.list == null ? 'Not connected to the PC.' : null));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, error: state.list == null ? e.shown : null, notice: state.list == null ? null : e.shown));
    }
  }

  /// A new item in the shown scope. Throws [ProtocolException] or
  /// [NotSentException]: the editor stays open with the reason.
  Future<VaultItemView> create(VaultCreateParams params) async {
    final item = await _repository.createVaultItem(params);
    add(const VaultRefreshed());
    return item;
  }

  /// Change [item] (still at its version).
  Future<VaultItemView> update(VaultItemView item, VaultUpdateParams params) async {
    final updated = await _repository.updateVaultItem(params, version: item.version);
    add(const VaultRefreshed());
    return updated;
  }

  /// "Delete" [item]; its history stays.
  Future<void> archive(VaultItemView item) async {
    await _repository.archiveVaultItem(item.id);
    add(const VaultRefreshed());
  }

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
