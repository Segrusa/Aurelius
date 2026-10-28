import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../../core/widgets/ios.dart';

/// The screenshot id in a screenshot tool's output (`… Saved as screenshot ID.`),
/// as on the desktop; `null` when there is none (or it is not a UUID).
String? screenshotId(String? output) {
  if (output == null) return null;
  final match = RegExp(r'Saved as screenshot ([0-9a-fA-F-]{36})(?![0-9a-fA-F-])').firstMatch(output);
  return match?.group(1);
}

/// A screenshot the agent took, full screen; pinch to zoom.
class ScreenshotPage extends StatefulWidget {
  const ScreenshotPage({super.key, required this.id});

  final String id;

  static Future<void> open(BuildContext context, String id) {
    final repository = context.read<AureliusRepository>();
    return Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => RepositoryProvider.value(value: repository, child: ScreenshotPage(id: id)),
    ));
  }

  @override
  State<ScreenshotPage> createState() => _ScreenshotPageState();
}

class _ScreenshotPageState extends State<ScreenshotPage> {
  late final Future<Uint8List> _image = context.read<AureliusRepository>().screenshot(widget.id);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Tokens.bg,
      appBar: GlassAppBar(title: const Text('Screenshot')),
      body: FutureBuilder<Uint8List>(
        future: _image,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text('This screenshot could not be loaded: ${snapshot.error}', textAlign: TextAlign.center, style: TextStyle(color: Tokens.muted)),
              ),
            );
          }
          final bytes = snapshot.data;
          if (bytes == null) return const Center(child: CircularProgressIndicator());
          return InteractiveViewer(
            maxScale: 6,
            child: Center(child: Image.memory(bytes, key: const Key('screenshot-image'), fit: BoxFit.contain)),
          );
        },
      ),
      bottomNavigationBar: SafeArea(
        child: Padding(
          padding: EdgeInsets.fromLTRB(16, 8, 16, 12),
          child: Text('What the agent saw, scaled down as it was sent. Kept on the PC (the last 300).', style: TextStyle(color: Tokens.muted, fontSize: 12)),
        ),
      ),
    );
  }
}
