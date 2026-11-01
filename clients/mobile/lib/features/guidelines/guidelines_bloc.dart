import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/guidelines.dart';
import '../../data/views.dart';

sealed class GuidelinesEvent {
  const GuidelinesEvent();
}

class GuidelinesRefreshed extends GuidelinesEvent {
  const GuidelinesRefreshed();
}

/// The search field (title, case does not count).
class GuidelinesQueryChanged extends GuidelinesEvent {
  const GuidelinesQueryChanged(this.query);

  final String query;
}

/// Got it on the seed banner (ui §8.1).
class GuidelinesBannerDismissed extends GuidelinesEvent {
  const GuidelinesBannerDismissed();
}

class GuidelinesNoticeShown extends GuidelinesEvent {
  const GuidelinesNoticeShown();
}

class GuidelinesState extends Equatable {
  const GuidelinesState({
    this.loading = true,
    this.active,
    this.archived = const [],
    this.proposals = const [],
    this.query = '',
    this.bannerDismissed = true,
    this.error,
    this.notice,
  });

  final bool loading;

  /// The active guidelines; null before the first answer.
  final List<GuidelineView>? active;

  /// The archived ones (GL-04 AC2), apart.
  final List<GuidelineView> archived;

  /// Pending proposals of `propose_guideline` (B5).
  final List<GuidelineProposalView> proposals;
  final String query;

  /// This board member said Got it to the seed banner (or it was never read yet).
  final bool bannerDismissed;
  final String? error;
  final String? notice;

  bool _matches(GuidelineView g) => query.isEmpty || g.title.toLowerCase().contains(query.toLowerCase());

  List<GuidelineView> get shown => (active ?? const []).where(_matches).toList();
  List<GuidelineView> get shownArchived => archived.where(_matches).toList();

  /// Nothing in the library at all, archived ones aside (ui §8.2).
  bool get empty => active != null && active!.isEmpty;

  /// The seed made guidelines nobody changed yet, and Got it was not said (B7).
  bool get showSeedBanner => !bannerDismissed && (active ?? const []).any(fromStarterSet);

  /// A guideline by id, active or archived.
  GuidelineView? guideline(String id) => [...?active, ...archived].where((g) => g.id == id).firstOrNull;

  /// The pending proposals for [guidelineId].
  List<GuidelineProposalView> proposalsFor(String guidelineId) => proposals.where((p) => p.guidelineId == guidelineId).toList();

  GuidelinesState copyWith({bool? loading, List<GuidelineView>? active, List<GuidelineView>? archived, List<GuidelineProposalView>? proposals, String? query, bool? bannerDismissed, String? error, String? notice}) =>
      GuidelinesState(
        loading: loading ?? this.loading,
        active: active ?? this.active,
        archived: archived ?? this.archived,
        proposals: proposals ?? this.proposals,
        query: query ?? this.query,
        bannerDismissed: bannerDismissed ?? this.bannerDismissed,
        error: error,
        notice: notice,
      );

  @override
  List<Object?> get props => [loading, active, archived, proposals, query, bannerDismissed, error, notice];
}

/// The block sizes `guideline.effective` gives per context, asked once per
/// screen and forgotten when a guideline changes.
class GuidelineSizes {
  GuidelineSizes(this._repository);

  final AureliusRepository _repository;
  final _sets = <BlockContext, Future<EffectiveGuidelines>>{};

  /// A failed read is asked again next time.
  Future<EffectiveGuidelines> of(BlockContext ctx) {
    final known = _sets[ctx];
    if (known != null) return known;
    final set = _repository.effectiveGuidelines(employeeId: ctx.employeeId, projectId: ctx.projectId);
    _sets[ctx] = set;
    set.then((_) {}, onError: (Object _) {
      if (identical(_sets[ctx], set)) _sets.remove(ctx);
    });
    return set;
  }

  /// The block of each context with guideline [guidelineId] at [withChars]
  /// characters (null: without it). A context that cannot be read is left out:
  /// the server checks again on save.
  Future<List<(BlockContext, int)>> blocks(Iterable<BlockContext> contexts, String guidelineId, int? withChars) async {
    final out = <(BlockContext, int)>[];
    await Future.wait([
      for (final ctx in contexts)
        of(ctx).then((set) => out.add((ctx, blockWith(set, guidelineId, withChars))), onError: (Object _) {}),
    ]);
    return out;
  }

  void clear() => _sets.clear();
}

/// The guideline library (More → Guidelines), kept fresh while its screen is
/// open: the bloc lives with the page, so nothing is read in the background
/// (C2c). Writes are domain commands for board members (B5); each returns
/// once the PC answered and throws [ProtocolException] or `NotSentException`.
class GuidelinesBloc extends Bloc<GuidelinesEvent, GuidelinesState> {
  GuidelinesBloc(this._repository) : super(const GuidelinesState()) {
    on<GuidelinesRefreshed>(_refresh, transformer: debounceRestartable(const Duration(milliseconds: 150)));
    on<GuidelinesQueryChanged>((e, emit) => emit(state.copyWith(query: e.query.trim())));
    on<GuidelinesBannerDismissed>(_dismissBanner);
    on<GuidelinesNoticeShown>((e, emit) => emit(state.copyWith()));
    _subs = [
      _repository.online.listen((_) => add(const GuidelinesRefreshed())),
      _repository.guidelineChanges.listen((_) {
        sizes.clear();
        add(const GuidelinesRefreshed());
      }),
    ];
    if (_repository.linkState.isOnline) add(const GuidelinesRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<void>> _subs;

  /// Block sizes for the editor and the link list.
  late final sizes = GuidelineSizes(_repository);

  AureliusRepository get repository => _repository;

  /// Only a signed-in board member with the right to change work edits (B5).
  bool get canEdit => _repository.linkState.user != null && _repository.linkState.can('task_control');

  Future<void> _refresh(GuidelinesRefreshed event, Emitter<GuidelinesState> emit) async {
    try {
      final active = await _repository.guidelines();
      final archived = await _repository.guidelines(archived: true);
      final proposals = await _repository.guidelineProposalList(status: ProposalStatus.pending);
      final user = _repository.linkState.user?.id;
      final dismissed = user == null || await SeedBannerMemory.dismissed(_repository.host.hostId, user);
      emit(state.copyWith(loading: false, active: active, archived: archived, proposals: proposals, bannerDismissed: dismissed));
    } on DisconnectedException {
      emit(state.copyWith(loading: false, error: state.active == null ? 'Not connected to the PC.' : null));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, error: state.active == null ? e.shown : null, notice: state.active == null ? null : e.shown));
    }
  }

  Future<void> _dismissBanner(GuidelinesBannerDismissed event, Emitter<GuidelinesState> emit) async {
    emit(state.copyWith(bannerDismissed: true));
    final user = _repository.linkState.user?.id;
    if (user != null) await SeedBannerMemory.dismiss(_repository.host.hostId, user);
  }

  /// The guideline id and version a write answered with.
  (String, int)? _changed(Receipt receipt) => switch (receipt.command) {
        CommandResultGuidelineChanged(:final guidelineId, :final version) => (guidelineId, version),
        _ => null,
      };

  Future<(String, int)?> _send(Map<String, dynamic> request, {int? expectedVersion}) async {
    final receipt = await _repository.write(request, expectedVersion: expectedVersion, retry: false);
    sizes.clear();
    add(const GuidelinesRefreshed());
    return _changed(receipt);
  }

  /// A new guideline (version 1), linked at once to [links]. Returns its id.
  Future<String?> create(String title, String text, {List<GuidelineTarget> links = const []}) async =>
      (await _send(guidelineCreate(title: title, text: text, links: links)))?.$1;

  /// A new version of [g] with what changed, from the version the editor started at.
  Future<int?> update(GuidelineView g, {String? title, String? text, required int fromVersion}) async =>
      (await _send(guidelineUpdate(guidelineId: g.id, title: title, text: text), expectedVersion: fromVersion))?.$2;

  /// No run gets it any more; links and versions stay (P-60).
  Future<void> archive(String guidelineId) => _send(guidelineArchive(guidelineId: guidelineId));

  /// Back from the archive with its links (B4 and the title are checked again).
  Future<void> restore(String guidelineId) => _send(guidelineRestore(guidelineId: guidelineId));

  /// A copy of [version] as the newest version (GL-03).
  Future<int?> restoreVersion(String guidelineId, int version) async => (await _send(guidelineRestoreVersion(guidelineId: guidelineId, version: version)))?.$2;

  /// Link what [after] has and [before] had not, unlink the rest, one by one.
  /// Stops at the first refusal (the server's reason), with what went through kept.
  Future<void> setLinks(String guidelineId, Set<String> before, Set<String> after) async {
    for (final key in after.difference(before)) {
      await _repository.write(guidelineLink(guidelineId: guidelineId, target: targetOf(key)), retry: false);
    }
    for (final key in before.difference(after)) {
      await _repository.write(guidelineUnlink(guidelineId: guidelineId, target: targetOf(key)), retry: false);
    }
    sizes.clear();
    add(const GuidelinesRefreshed());
  }

  /// The chosen templates as guidelines without links (B7). Returns how many were made.
  Future<int> addStarterSet(List<String> keys) async {
    final receipt = await _repository.write(guidelineAddStarterSet(keys: keys), retry: false);
    add(const GuidelinesRefreshed());
    return switch (receipt.command) { CommandResultStarterGuidelinesAdded(:final guidelineIds) => guidelineIds.length, _ => 0 };
  }

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
