import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/hosts/host.dart';
import '../../core/hosts/host_store.dart';

sealed class HostsEvent {
  const HostsEvent();
}

class HostsStarted extends HostsEvent {
  const HostsStarted();
}

class HostPaired extends HostsEvent {
  const HostPaired(this.host, this.deviceSecret);

  final Host host;
  final String deviceSecret;
}

class HostSelected extends HostsEvent {
  const HostSelected(this.hostId);

  final String hostId;
}

/// Forget a host on this phone (the PC keeps the device until revoked there).
class HostForgotten extends HostsEvent {
  const HostForgotten(this.hostId);

  final String hostId;
}

/// The store changed under the bloc: a host got the server's new name
/// (`server.renamed` or a hello, SN-02).
class HostsChanged extends HostsEvent {
  const HostsChanged();
}

class HostsState extends Equatable {
  const HostsState({this.loading = true, this.hosts = const [], this.activeId});

  final bool loading;
  final List<Host> hosts;
  final String? activeId;

  Host? get active {
    for (final h in hosts) {
      if (h.hostId == activeId) return h;
    }
    return hosts.isEmpty ? null : hosts.first;
  }

  @override
  List<Object?> get props => [loading, hosts, activeId];
}

/// Paired PCs and which one is active. Switching hosts rebuilds every
/// host-scoped read model (spec R08: no mixing of IDs between hosts).
class HostsBloc extends Bloc<HostsEvent, HostsState> {
  HostsBloc(this._store) : super(const HostsState()) {
    on<HostsStarted>((event, emit) async => emit(await _load()));
    on<HostsChanged>((event, emit) async => emit(await _load()));
    on<HostPaired>((event, emit) async {
      await _store.save(event.host, event.deviceSecret);
      emit(await _load());
    });
    on<HostSelected>((event, emit) async {
      await _store.setActive(event.hostId);
      emit(await _load());
    });
    on<HostForgotten>((event, emit) async {
      await _store.remove(event.hostId);
      emit(await _load());
    });
  }

  final HostStore _store;

  Future<HostsState> _load() async => HostsState(loading: false, hosts: await _store.hosts(), activeId: await _store.activeHostId());
}
