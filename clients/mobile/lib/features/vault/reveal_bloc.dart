import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/secure/secure_surface.dart';
import '../../data/aurelius_repository.dart';
import '../../data/vault.dart';
import '../../data/views.dart';

sealed class RevealEvent {
  const RevealEvent();
}

/// Show a secret's value for [revealFor] (audited as reveal). The page
/// sends it only after the confirmation (MU4-10 AC2).
class RevealShown extends RevealEvent {
  const RevealShown(this.itemId);

  final String itemId;
}

/// Hide it now: Hide, leaving the screen, the app going to the background.
class RevealHidden extends RevealEvent {
  const RevealHidden();
}

/// Copy a secret without showing it (audited as copy); the clipboard is
/// cleared after [clipboardClearAfter] if it still holds it.
class RevealCopied extends RevealEvent {
  const RevealCopied(this.itemId);

  final String itemId;
}

class RevealNoticeShown extends RevealEvent {
  const RevealNoticeShown();
}

class _Ticked extends RevealEvent {
  const _Ticked();
}

/// One item's reveal. [value] is there only while it shows: hidden, the
/// state holds no value at all (E19). Never printed.
class RevealState extends Equatable {
  const RevealState({this.itemId, this.value, this.secondsLeft = 0, this.busy = false, this.notice, this.hidden = false});

  /// The item whose value shows.
  final String? itemId;
  final String? value;

  /// Until it hides again ("Hides in 0:27").
  final int secondsLeft;

  /// Waiting for the PC's answer.
  final bool busy;
  final String? notice;

  /// It showed and was hidden again (Hide, the countdown, leaving): what a
  /// screen reader hears once ("Hidden", ui §6).
  final bool hidden;

  bool get shown => value != null;

  @override
  List<Object?> get props => [itemId, value, secondsLeft, busy, notice, hidden];

  // Equatable would print the props, the value among them.
  @override
  String toString() => 'RevealState(shown: $shown, secondsLeft: $secondsLeft, busy: $busy)';
}

/// The value of one secret on the item page, and copies of it. Asks the PC
/// for the value on each Show and Copy (each one is an audit row) and keeps
/// it nowhere else: no cache, no log, no debug print (E19). While it shows
/// the screen is secure ([SecureSurface.setSecure]).
class RevealBloc extends Bloc<RevealEvent, RevealState> {
  RevealBloc(this._repository, {SensitiveClipboard? clipboard, this.showFor = revealFor, this.clearAfter = clipboardClearAfter})
      : _clipboard = clipboard ?? SensitiveClipboard.instance,
        super(const RevealState()) {
    on<RevealShown>(_show);
    on<RevealHidden>((e, emit) => _hide(emit));
    on<RevealCopied>(_copy);
    on<_Ticked>(_tick);
    on<RevealNoticeShown>((e, emit) => emit(RevealState(itemId: state.itemId, value: state.value, secondsLeft: state.secondsLeft, busy: state.busy, hidden: state.hidden)));
  }

  final AureliusRepository _repository;
  final SensitiveClipboard _clipboard;
  final Duration showFor;
  final Duration clearAfter;
  Timer? _ticker;

  Future<void> _show(RevealShown event, Emitter<RevealState> emit) async {
    _ticker?.cancel();
    // The value that showed goes now; the screen is secure again only once a new one comes (M-m3).
    if (state.shown) unawaited(_clipboard.surface.setSecure(false));
    emit(const RevealState(busy: true));
    try {
      final value = await _repository.revealVaultValue(event.itemId, RevealPurpose.show);
      if (isClosed) return;
      // Secure before the value is drawn.
      await _clipboard.surface.setSecure(true);
      emit(RevealState(itemId: event.itemId, value: value, secondsLeft: showFor.inSeconds));
      _ticker = Timer.periodic(const Duration(seconds: 1), (_) => add(const _Ticked()));
    } on ProtocolException catch (e) {
      emit(RevealState(notice: revealFailure(e)));
    } on DisconnectedException catch (e) {
      emit(RevealState(notice: e.message));
    } catch (_) {
      // A timeout or anything else: never left busy.
      if (!isClosed) emit(const RevealState(notice: couldNotShow));
    }
  }

  void _tick(_Ticked event, Emitter<RevealState> emit) {
    if (!state.shown) return;
    final left = state.secondsLeft - 1;
    if (left <= 0) return _hide(emit);
    emit(RevealState(itemId: state.itemId, value: state.value, secondsLeft: left));
  }

  void _hide(Emitter<RevealState> emit) {
    _ticker?.cancel();
    _ticker = null;
    if (!state.shown) return;
    emit(const RevealState(hidden: true));
    unawaited(_clipboard.surface.setSecure(false));
  }

  Future<void> _copy(RevealCopied event, Emitter<RevealState> emit) async {
    final shown = state;
    emit(RevealState(itemId: shown.itemId, value: shown.value, secondsLeft: shown.secondsLeft, busy: true, hidden: shown.hidden));
    String? notice;
    try {
      final copied = await _clipboard.copy(await _repository.revealVaultValue(event.itemId, RevealPurpose.copy), clearAfter);
      notice = copied ? 'Copied. Clears in ${clearAfter.inSeconds} s.' : 'Could not copy.';
    } on ProtocolException catch (e) {
      notice = revealFailure(e);
    } on DisconnectedException catch (e) {
      notice = e.message;
    } catch (_) {
      notice = 'Could not copy.';
    }
    if (isClosed) return;
    emit(RevealState(itemId: state.itemId, value: state.value, secondsLeft: state.secondsLeft, notice: notice, hidden: state.hidden));
  }

  @override
  Future<void> close() {
    _ticker?.cancel();
    if (state.shown) unawaited(_clipboard.surface.setSecure(false));
    return super.close();
  }
}

const couldNotShow = 'Could not show the value. Try again.';

/// What to say when the PC does not give the value.
String revealFailure(ProtocolException e) => e.code == ErrorCode.vaultUndecryptable ? 'Cannot be decrypted: the vault key on the PC does not fit.' : e.shown;
