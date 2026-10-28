import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../../core/hosts/host.dart';
import '../../core/theme/tokens.dart';
import '../hosts/hosts_bloc.dart';
import 'pairing_bloc.dart';
import '../../core/widgets/ios.dart';

/// Pair this phone with a PC: scan the QR code from Aurelius on the PC
/// (Settings → Remote access → Pair a phone), or type address and code.
class PairPage extends StatefulWidget {
  const PairPage({super.key, required this.firstRun});

  final bool firstRun;

  @override
  State<PairPage> createState() => _PairPageState();
}

class _PairPageState extends State<PairPage> {
  final _name = TextEditingController(text: Platform.isIOS ? 'iPhone' : 'Android phone');
  bool _scanning = false;
  bool _manual = false;
  final _url = TextEditingController();
  final _code = TextEditingController();

  @override
  void dispose() {
    _name.dispose();
    _url.dispose();
    _code.dispose();
    super.dispose();
  }

  void _submit(PairingPayload payload) {
    setState(() => _scanning = false);
    final name = _name.text.trim().isEmpty ? 'Phone' : _name.text.trim();
    context.read<PairingBloc>().add(PairingSubmitted(payload, deviceName: name));
  }

  void _submitManual() {
    var url = _url.text.trim();
    if (url.isEmpty || _code.text.trim().isEmpty) return;
    if (!url.contains('://')) url = 'https://$url';
    final host = Uri.tryParse(url)?.host ?? '';
    final isIp = RegExp(r'^[\d.:]+$').hasMatch(host);
    final name = host.isEmpty || isIp ? 'the PC' : host.split('.').first;
    _submit(PairingPayload(url: url, code: _code.text.trim(), hostId: '', hostName: name, environment: 'production'));
  }

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<PairingBloc, PairingState>(
      listener: (context, state) {
        if (state.step == PairingStep.paired && state.host != null) {
          context.read<HostsBloc>().add(HostPaired(state.host!, state.deviceSecret!));
          if (!widget.firstRun) Navigator.of(context).maybePop();
        }
      },
      builder: (context, state) {
        return Scaffold(
          appBar: widget.firstRun ? null : GlassAppBar(title: const Text('Pair a PC')),
          body: SafeArea(
            child: switch (state.step) {
              PairingStep.submitting || PairingStep.waitingForDesktop => _Waiting(state: state),
              PairingStep.paired => const Center(child: CircularProgressIndicator()),
              _ => _scanning ? _scanner() : _intro(state),
            },
          ),
        );
      },
    );
  }

  Widget _scanner() {
    return Stack(
      children: [
        MobileScanner(
          onDetect: (capture) {
            for (final code in capture.barcodes) {
              final payload = PairingPayload.tryParse(code.rawValue ?? '');
              if (payload != null) {
                _submit(payload);
                return;
              }
            }
          },
        ),
        Positioned(
          left: 16,
          right: 16,
          bottom: 24,
          child: Column(
            children: [
              const Text('Point the camera at the QR code on the PC.', style: TextStyle(color: Colors.white, fontSize: 15), textAlign: TextAlign.center),
              const SizedBox(height: 12),
              OutlinedButton(onPressed: () => setState(() => _scanning = false), child: const Text('Cancel')),
            ],
          ),
        ),
      ],
    );
  }

  Widget _intro(PairingState state) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(24, 32, 24, 24),
      children: [
        if (widget.firstRun) ...[
          Center(
            child: CustomPaint(size: const Size(56, 56), painter: _LogoPainter()),
          ),
          const SizedBox(height: 20),
          Text('Aurelius', textAlign: TextAlign.center, style: Tokens.title(size: 34)),
          const SizedBox(height: 8),
        ],
        Text(
          'Your PC stays in charge: it keeps projects, memory and credentials. '
          'This phone becomes another window onto the same chats, tasks and approvals, over your Tailscale network.',
          textAlign: TextAlign.center,
          style: TextStyle(color: Tokens.text2, fontSize: 14.5, height: 1.5),
        ),
        const SizedBox(height: 28),
        const _Step(n: 1, text: 'On the PC, open Aurelius → Settings → Remote access and turn it on.'),
        const _Step(n: 2, text: 'Choose "Pair a phone". A QR code appears.'),
        const _Step(n: 3, text: 'Scan it here, then press Allow on the PC.'),
        const SizedBox(height: 20),
        TextField(controller: _name, decoration: const InputDecoration(labelText: 'Name of this phone')),
        const SizedBox(height: 16),
        if (state.step == PairingStep.failed && state.message != null) ...[
          Text(state.message!, style: TextStyle(color: Tokens.negative)),
          const SizedBox(height: 12),
        ],
        FilledButton.icon(
          onPressed: () => setState(() => _scanning = true),
          icon: const Icon(Icons.qr_code_scanner),
          label: const Text('Scan QR code'),
        ),
        const SizedBox(height: 8),
        TextButton(onPressed: () => setState(() => _manual = !_manual), child: Text(_manual ? 'Hide manual entry' : 'Enter address and code instead')),
        if (_manual) ...[
          TextField(
            controller: _url,
            keyboardType: TextInputType.url,
            autocorrect: false,
            decoration: const InputDecoration(labelText: 'Address', hintText: 'desktop.tail1234.ts.net:8443'),
          ),
          const SizedBox(height: 10),
          TextField(
            controller: _code,
            autocorrect: false,
            textCapitalization: TextCapitalization.characters,
            decoration: const InputDecoration(labelText: 'Code', hintText: 'K7M2-9QXA'),
            onSubmitted: (_) => _submitManual(),
          ),
          const SizedBox(height: 12),
          OutlinedButton(onPressed: _submitManual, child: const Text('Pair')),
        ],
      ],
    );
  }
}

class _Step extends StatelessWidget {
  const _Step({required this.n, required this.text});

  final int n;
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(bottom: 12),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              width: 22,
              height: 22,
              alignment: Alignment.center,
              decoration: BoxDecoration(shape: BoxShape.circle, border: Border.all(color: Tokens.borderStrong)),
              child: Text('$n', style: TextStyle(fontSize: 12, color: Tokens.text2)),
            ),
            const SizedBox(width: 12),
            Expanded(child: Text(text, style: const TextStyle(fontSize: 14.5, height: 1.4))),
          ],
        ),
      );
}

class _Waiting extends StatelessWidget {
  const _Waiting({required this.state});

  final PairingState state;

  @override
  Widget build(BuildContext context) {
    final waiting = state.step == PairingStep.waitingForDesktop;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const CircularProgressIndicator(),
            const SizedBox(height: 24),
            Text(
              waiting ? 'Confirm on ${state.hostName ?? 'the PC'}' : 'Contacting ${state.hostName ?? 'the PC'}…',
              textAlign: TextAlign.center,
              style: Tokens.title(size: 24),
            ),
            const SizedBox(height: 10),
            if (waiting)
              Text('Aurelius on the PC asks whether this phone may connect. Press Allow there.',
                  textAlign: TextAlign.center, style: TextStyle(color: Tokens.text2)),
            const SizedBox(height: 24),
            TextButton(onPressed: () => context.read<PairingBloc>().add(const PairingReset()), child: const Text('Cancel')),
          ],
        ),
      ),
    );
  }
}

/// The Aurelius mark: `M60 14L104 106H82L60 56L38 106H16Z` on a 120 grid.
class _LogoPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width / 120;
    final path = Path()
      ..moveTo(60 * s, 14 * s)
      ..lineTo(104 * s, 106 * s)
      ..lineTo(82 * s, 106 * s)
      ..lineTo(60 * s, 56 * s)
      ..lineTo(38 * s, 106 * s)
      ..lineTo(16 * s, 106 * s)
      ..close();
    canvas.drawPath(path, Paint()..color = Tokens.text);
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}
