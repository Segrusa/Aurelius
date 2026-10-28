import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/hosts/host.dart';
import '../../core/net/gateway_api.dart';

sealed class PairingEvent {
  const PairingEvent();
}

/// A scanned QR code (raw text) or a typed address + code.
class PairingSubmitted extends PairingEvent {
  const PairingSubmitted(this.payload, {required this.deviceName});

  final PairingPayload payload;
  final String deviceName;
}

class PairingReset extends PairingEvent {
  const PairingReset();
}

enum PairingStep { idle, submitting, waitingForDesktop, paired, failed }

class PairingState extends Equatable {
  const PairingState({this.step = PairingStep.idle, this.hostName, this.message, this.host, this.deviceSecret});

  final PairingStep step;
  final String? hostName;
  final String? message;
  final Host? host;
  final String? deviceSecret;

  @override
  List<Object?> get props => [step, hostName, message, host, deviceSecret];
}

/// Pair with a PC: enter the one-time code, then wait until the desktop
/// allows this phone. The device secret goes straight to the Keychain via
/// `HostPaired`; it is never shown.
class PairingBloc extends Bloc<PairingEvent, PairingState> {
  PairingBloc(this._api, {required this.platform}) : super(const PairingState()) {
    on<PairingSubmitted>(_submit, transformer: droppable());
    on<PairingReset>((event, emit) => emit(const PairingState()));
  }

  final GatewayApi _api;

  /// `ios` or `android`.
  final String platform;

  Future<void> _submit(PairingSubmitted event, Emitter<PairingState> emit) async {
    final payload = event.payload;
    emit(PairingState(step: PairingStep.submitting, hostName: payload.hostName));
    try {
      final pairingId = await _api.pair(payload, deviceName: event.deviceName, platform: platform);
      emit(PairingState(step: PairingStep.waitingForDesktop, hostName: payload.hostName));
      final deadline = DateTime.now().add(const Duration(minutes: 5));
      while (!isClosed && DateTime.now().isBefore(deadline)) {
        final result = await _api.pollPairing(payload.url, pairingId);
        switch (result) {
          case PairingPending():
            continue;
          case PairingApproved():
            final host = Host(
              hostId: result.hostId,
              name: result.hostName ?? payload.hostName,
              url: payload.url,
              clientId: result.clientId,
              environment: result.environment,
              pairedAtMs: DateTime.now().millisecondsSinceEpoch,
            );
            emit(PairingState(step: PairingStep.paired, hostName: payload.hostName, host: host, deviceSecret: result.deviceSecret));
            return;
          case PairingRejected():
            emit(PairingState(step: PairingStep.failed, hostName: payload.hostName, message: 'The PC refused this phone.'));
            return;
          case PairingExpired():
            break;
        }
        break;
      }
      emit(PairingState(step: PairingStep.failed, hostName: payload.hostName, message: 'The request expired. Show a new code on the PC.'));
    } on GatewayException catch (e) {
      emit(PairingState(step: PairingStep.failed, hostName: payload.hostName, message: e.message));
    } on HostUnreachableException catch (e) {
      emit(PairingState(step: PairingStep.failed, hostName: payload.hostName, message: e.message));
    }
  }
}
