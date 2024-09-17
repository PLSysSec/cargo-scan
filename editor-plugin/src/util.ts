import * as vscode from 'vscode';
import { execSync } from 'child_process';
import * as path from 'path';
import { homedir } from 'os';

export function convertLocation(obj: any): vscode.Location {
    const uri = vscode.Uri.parse(obj.uri.toString());
    const range = new vscode.Range(
        new vscode.Position(obj.range.start.line, obj.range.start.character),
        new vscode.Position(obj.range.end.line, obj.range.end.character)
    );

    // `fsPath` is lazily initialized, so we access it here to trigger its initialization. 
    //  We need it to correctly stringify locations when handling a base effect's callstack.
    uri.fsPath;

    return new vscode.Location(uri, range);
}

export function setEnvironment(config: vscode.WorkspaceConfiguration) {
    let rustPath = config.get<string>('rustPath');

    // If `rustPath` is not set in the extension configurations,
    // determine default Rust installation paths based on platform
    if (!rustPath || rustPath.trim().length === 0) {
        rustPath = process.platform === 'win32' ?
            path.join(process.env.USERPROFILE || '', '.cargo', 'bin') :
            path.join(homedir(), '.cargo', 'bin');
    }
    
    // Export Rust toolchain to `$PATH`
    process.env.PATH = `${rustPath}${path.delimiter}${process.env.PATH || ''}`;
    checkRustToolchain(rustPath);

    // Set RUST_LOG environment variable to the configured log level
    const level = config.get<string>('log.level');
    process.env.RUST_LOG = level;
}

function checkRustToolchain(rustPath: string) {
    try {
        // Verify rustc can be executed
        execSync(`${path.join(rustPath, 'rustc')} -vV`, { encoding: 'utf8' }).trim();

    } catch (error) {
        vscode.window.showErrorMessage(`Failed to set environment: Could not find Rust 
            toolchain in "${rustPath}". Try setting the path in the extension Settings.`); 
    }
}