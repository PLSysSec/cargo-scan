import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';
import {
    LocationsProvider,
} from './file_tree_view';
import { registerCommands } from './commands';
import { AuditAnnotations } from './audit_annotations';
import { setEnvironment } from './util';

export let client: LanguageClient;
export const locationsProvider = new LocationsProvider();
export const annotations = new AuditAnnotations();

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

    const config = vscode.workspace.getConfiguration('cargo-scan');
    const serverModule = config.get<string>('serverPath');
    setEnvironment(config);
    
    let serverOptions: ServerOptions = {
        command: serverModule && serverModule.trim().length !== 0
            ? serverModule
            : context.asAbsolutePath(path.join("out", "lang_server")),
        args: [],
        options: {
            env: { ...process.env },
        },
        transport: TransportKind.stdio,
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
    
    
    // Register everything
    registerCommands(context);
    annotations.register(context);
    locationsProvider.register(context);    
}

export function deactivate() { 
    return new Promise<void>((resolve) => {
        client.sendRequest('shutdown').then(() => {
            client.sendNotification('exit');
            resolve();
        });
    });
}
