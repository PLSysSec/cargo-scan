import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';
import {
    EffectsResponse,
    LocationsProvider,
} from './file_tree_view';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
    const outputChannel = vscode.window.createOutputChannel('Cargo Scan Client');

    if (!vscode.workspace.workspaceFolders) {
        outputChannel.appendLine('No workspace folders found.');
        return;
    }

    const workspace_folders = vscode.workspace.workspaceFolders.map((folder) => ({
        uri: folder.uri.toString(),
        name: folder.name,
    }));

    let serverModule = context.asAbsolutePath(
        path.join('..', 'target', 'debug', 'lang_server')
    );
    outputChannel.appendLine(`server mod: ${serverModule}`);
    let serverOptions: ServerOptions = {
        command: serverModule,
        args: [],
        options: {
            env: { ...process.env, RUST_LOG: 'info' },
        },
        // transport: TransportKind.stdio,
    };

    let clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'rust' }],
        initializationOptions: {
            workspace_folders: workspace_folders,
        },
    };

    client = new LanguageClient(
        'lspserver',
        'Cargo Scan Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
    outputChannel.appendLine('Cargo Scan extension is now active!');

    // Register the tree view provider
    const locationsProvider = new LocationsProvider(client);
    locationsProvider.register(context);

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.scan', async () => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.scan');
            context.globalState.update('annotateEffects', false);
            locationsProvider.setLocations(response.effects);
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.audit', async () => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.audit');
            context.globalState.update('annotateEffects', true);
            locationsProvider.setLocations(response.effects);
        })
    );
}

export function deactivate() { }
