import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:bloc_concurrency/bloc_concurrency.dart';

/// Refresh events: wait for a burst of daemon events to settle, then run
/// the latest only (a newer refresh cancels an older one in flight).
EventTransformer<E> debounceRestartable<E>([Duration wait = const Duration(milliseconds: 150)]) {
  return (events, mapper) => restartable<E>()(_debounce(events, wait), mapper);
}

Stream<E> _debounce<E>(Stream<E> source, Duration wait) {
  late StreamController<E> controller;
  Timer? timer;
  E? last;
  StreamSubscription<E>? sub;
  controller = StreamController<E>(
    sync: true,
    onListen: () {
      sub = source.listen(
        (event) {
          last = event;
          timer?.cancel();
          timer = Timer(wait, () => controller.add(last as E));
        },
        onError: controller.addError,
        onDone: () {
          timer?.cancel();
          controller.close();
        },
      );
    },
    onCancel: () {
      timer?.cancel();
      return sub?.cancel();
    },
  );
  return controller.stream;
}
