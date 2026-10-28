import 'dart:convert';
import 'dart:io';

import 'package:http/http.dart' as http;

import '../hosts/host.dart';

/// HTTP side of the gateway: pairing, sessions, uploads and push
/// registration. Credentials go only in the Authorization header.
class GatewayApi {
  GatewayApi({http.Client? client}) : _http = client ?? http.Client();

  final http.Client _http;
  static const _timeout = Duration(seconds: 15);

  /// Enter the code from the desktop. Returns the pairing id; the desktop
  /// must then allow this device.
  Future<String> pair(PairingPayload payload, {required String deviceName, required String platform}) async {
    final response = await _send(() => _http.post(
          Uri.parse(payload.url).resolve('/v1/pair'),
          headers: {'content-type': 'application/json'},
          body: jsonEncode({'code': payload.code, 'device_name': deviceName, 'platform': platform}),
        ));
    final body = _json(response);
    if (response.statusCode != 202) throw GatewayException(response.statusCode, body['error'] as String? ?? 'Pairing failed.');
    return body['pairing_id'] as String;
  }

  /// Long poll (≤25 s) for the desktop's decision.
  Future<PairingResult> pollPairing(String baseUrl, String pairingId) async {
    final response = await _send(
      () => _http.get(Uri.parse(baseUrl).resolve('/v1/pair/$pairingId')),
      timeout: const Duration(seconds: 35),
    );
    final body = _json(response);
    switch (response.statusCode) {
      case 200 when body['status'] == 'approved':
        return PairingApproved(
          clientId: body['client_id'] as String,
          deviceSecret: body['device_secret'] as String,
          hostId: body['host_id'] as String,
          hostName: body['host_name'] as String?,
          environment: body['environment'] as String? ?? 'production',
        );
      case 200:
        return const PairingPending();
      case 403:
        return const PairingRejected();
      default:
        return const PairingExpired();
    }
  }

  /// Trade the device secret for a session token.
  Future<Session> session(Host host, String deviceSecret) async {
    final basic = base64.encode(utf8.encode('${host.clientId}:$deviceSecret'));
    final response = await _send(() => _http.post(host.endpoint('/v1/session'), headers: {'authorization': 'Basic $basic'}));
    final body = _json(response);
    if (response.statusCode == 401) throw const RevokedException();
    if (response.statusCode != 200) throw GatewayException(response.statusCode, body['error'] as String? ?? 'No session.');
    return Session(
      token: body['token'] as String,
      expiresAtMs: (body['expires_at_ms'] as num).toInt(),
      capabilities: (body['capabilities'] as List<dynamic>? ?? const []).cast<String>(),
    );
  }

  /// Upload one file into this device's inbox on the PC; returns the host path
  /// to pass to `attach_file`.
  Future<UploadedFile> upload(Host host, String token, File file, String name) async {
    final request = http.MultipartRequest('POST', host.endpoint('/v1/upload'))
      ..headers['authorization'] = 'Bearer $token'
      ..files.add(await http.MultipartFile.fromPath('file', file.path, filename: name));
    final streamed = await _http.send(request).timeout(const Duration(minutes: 3));
    final response = await http.Response.fromStream(streamed);
    final body = _json(response);
    if (response.statusCode == 401) throw const SessionExpiredException();
    if (response.statusCode != 200) throw GatewayException(response.statusCode, body['error'] as String? ?? 'Upload failed.');
    return UploadedFile(path: body['path'] as String, name: body['name'] as String, byteLength: (body['byte_length'] as num).toInt());
  }

  Future<void> registerPush(Host host, String token, {required String platform, required String pushToken, required String environment}) async {
    final response = await _send(() => _http.put(
          host.endpoint('/v1/push'),
          headers: {'authorization': 'Bearer $token', 'content-type': 'application/json'},
          body: jsonEncode({'platform': platform, 'token': pushToken, 'environment': environment}),
        ));
    if (response.statusCode == 401) throw const SessionExpiredException();
    if (response.statusCode != 204) throw GatewayException(response.statusCode, 'Could not turn on notifications.');
  }

  Future<void> unregisterPush(Host host, String token) async {
    await _send(() => _http.delete(host.endpoint('/v1/push'), headers: {'authorization': 'Bearer $token'}));
  }

  Future<http.Response> _send(Future<http.Response> Function() send, {Duration timeout = _timeout}) async {
    try {
      return await send().timeout(timeout);
    } on SocketException {
      throw const HostUnreachableException();
    } on HttpException {
      throw const HostUnreachableException();
    } on http.ClientException {
      throw const HostUnreachableException();
    } on HandshakeException {
      throw const HostUnreachableException('The PC\'s certificate was refused. Check Tailscale HTTPS on the PC.');
    }
  }

  Map<String, dynamic> _json(http.Response response) {
    if (response.body.isEmpty) return const {};
    try {
      final decoded = jsonDecode(response.body);
      return decoded is Map<String, dynamic> ? decoded : const {};
    } on FormatException {
      return const {};
    }
  }
}

class Session {
  const Session({required this.token, required this.expiresAtMs, required this.capabilities});

  final String token;
  final int expiresAtMs;
  final List<String> capabilities;

  bool get expiresSoon => DateTime.now().millisecondsSinceEpoch > expiresAtMs - 5 * 60 * 1000;
}

class UploadedFile {
  const UploadedFile({required this.path, required this.name, required this.byteLength});

  final String path;
  final String name;
  final int byteLength;
}

sealed class PairingResult {
  const PairingResult();
}

class PairingPending extends PairingResult {
  const PairingPending();
}

class PairingApproved extends PairingResult {
  const PairingApproved({required this.clientId, required this.deviceSecret, required this.hostId, this.hostName, required this.environment});

  final String clientId;
  final String deviceSecret;
  final String hostId;

  /// The PC's own name (e.g. `my-pc`).
  final String? hostName;
  final String environment;
}

class PairingRejected extends PairingResult {
  const PairingRejected();
}

class PairingExpired extends PairingResult {
  const PairingExpired();
}

class GatewayException implements Exception {
  const GatewayException(this.status, this.message);

  final int status;
  final String message;

  @override
  String toString() => message;
}

class HostUnreachableException implements Exception {
  const HostUnreachableException([this.message = 'The PC is not reachable. Is it on, with Aurelius and Tailscale running?']);

  final String message;

  @override
  String toString() => message;
}

/// The device credential no longer works (revoked on the desktop).
class RevokedException implements Exception {
  const RevokedException();

  @override
  String toString() => 'This phone was removed on the PC. Pair again.';
}

class SessionExpiredException implements Exception {
  const SessionExpiredException();
}
