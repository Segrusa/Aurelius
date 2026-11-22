import 'dart:convert';

import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/models.dart';
import 'artifact_html.dart';
import 'artifact_versions.dart';

sealed class ArtifactEvent {
  const ArtifactEvent();
}

/// The conversation's messages changed: a new version may have come in.
class ArtifactMessagesChanged extends ArtifactEvent {
  const ArtifactMessagesChanged(this.messages);

  final List<MessageView> messages;
}

class ArtifactVersionSelected extends ArtifactEvent {
  const ArtifactVersionSelected(this.fileId);

  final String fileId;
}

class ArtifactViewportSelected extends ArtifactEvent {
  const ArtifactViewportSelected(this.viewport);

  final ArtifactViewport viewport;
}

/// Load the shown version again from the start, with a fresh nonce.
class ArtifactReloaded extends ArtifactEvent {
  const ArtifactReloaded();
}

/// After a failure: fetch the shown version from the server again.
class ArtifactRetried extends ArtifactEvent {
  const ArtifactRetried();
}

class ArtifactState extends Equatable {
  const ArtifactState({
    required this.versions,
    required this.fileId,
    this.viewport = ArtifactViewport.mobile,
    this.html,
    this.document,
    this.error,
    this.load = 0,
  });

  /// Newest first.
  final List<ArtifactVersion> versions;

  /// The version shown.
  final String fileId;
  final ArtifactViewport viewport;

  /// The shown version's HTML as the server keeps it (for Share HTML).
  final String? html;

  /// What the webview loads: [html] with the policy and a nonce of this load.
  final String? document;

  /// Why the shown version could not be fetched.
  final String? error;

  /// Counts the loads, so the webview loads again on Reload with the same HTML.
  final int load;

  ArtifactVersion get current => versions.firstWhere((v) => v.file.id == fileId, orElse: () => versions.first);

  bool get loading => document == null && error == null;

  @override
  List<Object?> get props => [versions, fileId, viewport, html, document, error, load];
}

/// One artifact opened from a kept file: its versions in the conversation, the
/// shown one fetched with `file.get` and wrapped for the webview.
class ArtifactBloc extends Bloc<ArtifactEvent, ArtifactState> {
  ArtifactBloc(this._repository, {required FileRef file, required List<MessageView> messages, required int openedAtMs, String Function()? nonce})
      : _file = file,
        _openedAtMs = openedAtMs,
        _nonce = nonce ?? artifactNonce,
        super(ArtifactState(versions: versionsFor(messages, file, openedAtMs), fileId: file.id)) {
    on<ArtifactMessagesChanged>(_messages);
    on<ArtifactVersionSelected>((e, emit) => _show(e.fileId, emit), transformer: restartable());
    on<ArtifactViewportSelected>((e, emit) => emit(_with(viewport: e.viewport)));
    on<ArtifactReloaded>(_reload);
    on<ArtifactRetried>((e, emit) => _show(state.fileId, emit, fetch: true), transformer: restartable());
    add(ArtifactVersionSelected(file.id));
  }

  final AureliusRepository _repository;
  final FileRef _file;
  final int _openedAtMs;
  final String Function() _nonce;

  /// The HTML of versions already fetched, by file id: a kept file never changes.
  final _fetched = <String, String>{};

  ArtifactState _with({List<ArtifactVersion>? versions, String? fileId, ArtifactViewport? viewport, String? html, String? document, String? error, int? load, bool clear = false}) =>
      ArtifactState(
        versions: versions ?? state.versions,
        fileId: fileId ?? state.fileId,
        viewport: viewport ?? state.viewport,
        html: clear ? html : html ?? state.html,
        document: clear ? document : document ?? state.document,
        error: clear ? error : error ?? state.error,
        load: load ?? state.load,
      );

  void _messages(ArtifactMessagesChanged event, Emitter<ArtifactState> emit) {
    final versions = versionsFor(event.messages, _file, _openedAtMs);
    // The shown version stays shown, even when its message was deleted meanwhile.
    final shown = versions.any((v) => v.file.id == state.fileId) ? versions : [...versions, state.current];
    emit(_with(versions: shown));
  }

  Future<void> _show(String fileId, Emitter<ArtifactState> emit, {bool fetch = false}) async {
    if (fetch) _fetched.remove(fileId);
    final kept = _fetched[fileId];
    if (kept != null) {
      emit(_with(fileId: fileId, html: kept, document: artifactDocument(kept, _nonce()), load: state.load + 1, clear: true));
      return;
    }
    emit(_with(fileId: fileId, load: state.load + 1, clear: true));
    try {
      final html = utf8.decode(await _repository.file(fileId), allowMalformed: true);
      _fetched[fileId] = html;
      if (state.fileId != fileId) return;
      emit(_with(html: html, document: artifactDocument(html, _nonce())));
    } on ProtocolException catch (e) {
      emit(_with(error: e.message));
    } on DisconnectedException catch (e) {
      emit(_with(error: e.message));
    } on FormatException {
      emit(_with(error: 'The server sent a file the app cannot read.'));
    }
  }

  void _reload(ArtifactReloaded event, Emitter<ArtifactState> emit) {
    final html = state.html;
    if (html == null) return;
    emit(_with(document: artifactDocument(html, _nonce()), load: state.load + 1));
  }
}
