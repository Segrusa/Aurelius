import 'dart:convert';

import 'package:equatable/equatable.dart';

/// A paired PC. IDs from one host are never mixed with another (spec R08):
/// every read model lives under the active host and is dropped on a switch.
class Host extends Equatable {
  const Host({
    required this.hostId,
    required this.name,
    required this.url,
    required this.clientId,
    required this.environment,
    required this.pairedAtMs,
  });

  factory Host.fromJson(Map<String, dynamic> json) => Host(
        hostId: json['host_id'] as String,
        name: json['name'] as String,
        url: json['url'] as String,
        clientId: json['client_id'] as String,
        environment: json['environment'] as String? ?? 'production',
        pairedAtMs: (json['paired_at_ms'] as num?)?.toInt() ?? 0,
      );

  final String hostId;

  /// The PC's name, e.g. `my-pc`.
  final String name;

  /// Gateway base URL, e.g. `https://my-pc.tail1234.ts.net:8443`.
  final String url;

  /// This phone's device id on that host.
  final String clientId;

  /// `production` or `dev`.
  final String environment;
  final int pairedAtMs;

  bool get isDev => environment == 'dev';

  Uri endpoint(String path) => Uri.parse(url).resolve(path);

  Uri get websocketUri {
    final base = Uri.parse(url);
    return base.replace(scheme: base.scheme == 'https' ? 'wss' : 'ws', path: '/v1/ws');
  }

  Map<String, dynamic> toJson() => {
        'host_id': hostId,
        'name': name,
        'url': url,
        'client_id': clientId,
        'environment': environment,
        'paired_at_ms': pairedAtMs,
      };

  @override
  List<Object?> get props => [hostId, name, url, clientId, environment, pairedAtMs];
}

/// What the desktop's QR code holds (`PairingPayload`, v1).
class PairingPayload extends Equatable {
  const PairingPayload({
    required this.url,
    required this.code,
    required this.hostId,
    required this.hostName,
    required this.environment,
  });

  /// Parse a scanned QR code; `null` when it is not an Aurelius code.
  static PairingPayload? tryParse(String raw) {
    try {
      final json = jsonDecode(raw);
      if (json is! Map<String, dynamic> || json['v'] != 1) return null;
      final url = json['url'] as String?;
      final code = json['code'] as String?;
      if (url == null || code == null || Uri.tryParse(url)?.hasScheme != true) return null;
      return PairingPayload(
        url: url,
        code: code,
        hostId: json['host_id'] as String? ?? '',
        hostName: json['host_name'] as String? ?? 'PC',
        environment: json['env'] as String? ?? 'production',
      );
    } on FormatException {
      return null;
    }
  }

  final String url;
  final String code;
  final String hostId;
  final String hostName;
  final String environment;

  @override
  List<Object?> get props => [url, code, hostId, hostName, environment];
}
