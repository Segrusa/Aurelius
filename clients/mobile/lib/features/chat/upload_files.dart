import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:open_filex/open_filex.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

/// Thumbnails are asked at this size: sharp at 3x in a ~80 pt tile.
const uploadThumbSide = 240;

bool isImageName(String name) => RegExp(r'\.(png|jpe?g|gif|webp|bmp|heic)$', caseSensitive: false).hasMatch(name);

/// The upload can be fetched from this server: it has an id and the server
/// knows `upload.get`. Older servers keep plain chips.
bool uploadFetchable(BuildContext context, MessageFileView file) => file.id != null && context.read<AureliusRepository>().uploadsFetchable;

/// Thumbnails fetched this session, by upload id (empty: the server has none).
final _thumbs = <String, Future<Uint8List>>{};

Future<Uint8List> _thumb(AureliusRepository repository, String id) {
  if (_thumbs.length > 200) _thumbs.remove(_thumbs.keys.first);
  return _thumbs.putIfAbsent(id, () => repository.uploadThumbnail(id, uploadThumbSide).catchError((Object _) => Uint8List(0)));
}

/// An uploaded image's thumbnail from the server; [fallback] while it loads or when there is none.
class UploadThumb extends StatelessWidget {
  const UploadThumb({super.key, required this.id, required this.width, required this.height, required this.fallback});

  final String id;
  final double width;
  final double height;
  final Widget fallback;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<Uint8List>(
      future: _thumb(context.read<AureliusRepository>(), id),
      builder: (context, snapshot) {
        final bytes = snapshot.data;
        if (bytes == null || bytes.isEmpty) return fallback;
        return ClipRRect(
          borderRadius: BorderRadius.circular(6),
          child: Image.memory(bytes, key: Key('upload-thumb-$id'), width: width, height: height, fit: BoxFit.cover, errorBuilder: (_, _, _) => fallback),
        );
      },
    );
  }
}

/// Open an uploaded file: an image in the app (pinch to zoom), anything else
/// with an app on the phone that handles it.
Future<void> openUpload(BuildContext context, MessageFileView file) async {
  final id = file.id;
  if (id == null) return;
  final repository = context.read<AureliusRepository>();
  if (isImageName(file.name)) {
    await Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => RepositoryProvider.value(value: repository, child: UploadImagePage(file: file)),
    ));
    return;
  }
  toast(context, 'Opening ${file.name}…');
  try {
    final bytes = await repository.fetchUpload(id);
    final safe = file.name.replaceAll(RegExp(r'[/\\:*?"<>|]'), '_').trim();
    final dir = Directory('${Directory.systemTemp.path}/aurelius-files/${id.replaceAll(RegExp(r'[^A-Za-z0-9_-]'), '_')}');
    await dir.create(recursive: true);
    final path = '${dir.path}/${safe.isEmpty ? 'file' : safe}';
    await File(path).writeAsBytes(bytes, flush: true);
    final result = await OpenFilex.open(path);
    if (result.type != ResultType.done && context.mounted) {
      toast(context, result.type == ResultType.noAppToOpen ? 'No app on this phone opens ${file.name}.' : 'Could not open ${file.name}: ${result.message}');
    }
  } catch (e) {
    if (context.mounted) toast(context, 'Could not open ${file.name}: $e');
  }
}

/// An uploaded image, fetched whole from the server.
class UploadImagePage extends StatefulWidget {
  const UploadImagePage({super.key, required this.file});

  final MessageFileView file;

  @override
  State<UploadImagePage> createState() => _UploadImagePageState();
}

class _UploadImagePageState extends State<UploadImagePage> {
  late final Future<Uint8List> _bytes = context.read<AureliusRepository>().fetchUpload(widget.file.id!);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Tokens.bg,
      appBar: GlassAppBar(title: Text(widget.file.name, overflow: TextOverflow.ellipsis)),
      body: FutureBuilder<Uint8List>(
        future: _bytes,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text('This file could not be loaded: ${snapshot.error}', textAlign: TextAlign.center, style: TextStyle(color: Tokens.muted)),
              ),
            );
          }
          final bytes = snapshot.data;
          if (bytes == null) return const Center(child: CircularProgressIndicator());
          return InteractiveViewer(
            maxScale: 6,
            child: Center(
              child: Image.memory(
                bytes,
                key: const Key('upload-image'),
                fit: BoxFit.contain,
                errorBuilder: (_, _, _) => Text('This image can\'t be shown on the phone.', style: TextStyle(color: Tokens.muted)),
              ),
            ),
          );
        },
      ),
    );
  }
}
