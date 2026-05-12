import * as vscode from 'vscode';
import { EffectResponseData } from './file_tree_view';

const highlightDecorationType = vscode.window.createTextEditorDecorationType({
    borderWidth: '1.5px',
	borderStyle: 'dotted',
    overviewRulerLane: vscode.OverviewRulerLane.Center,
    
    light: {
        // Color to be used in light color themes
        backgroundColor: 'rgba(50, 50, 50, 0.3)',
        overviewRulerColor: 'rgba(50, 50, 50, 0.3)',
    },
    dark: {
        // Color to be used in dark color themes
        backgroundColor: 'rgba(211, 211, 211, 0.3)',
        overviewRulerColor: 'rgba(211, 211, 211, 0.3)'
    }
});


export function highlightEffectLocations(
    editor: vscode.TextEditor,
    effects: { [file: string]: EffectResponseData[] }
) {
    const file = editor?.document.fileName;
    const fileEffects = effects[file];
    if (fileEffects) {
        const ranges = fileEffects.map( effect => effect.location.range );
        editor.setDecorations(highlightDecorationType, ranges);
    }
}


export class TreeDecorationProvider implements vscode.FileDecorationProvider {
    private decorations: { [uri: string]: number } = {};
    private chainDirs: string[] = [];
    private rankDecorations: Map<string, { own: number, incoming: number }> = new Map();

    _onDidChangeFileDecorations: vscode.EventEmitter<vscode.Uri | vscode.Uri[]> =
        new vscode.EventEmitter<vscode.Uri | vscode.Uri[]>();
	onDidChangeFileDecorations: vscode.Event<vscode.Uri | vscode.Uri[]> =
        this._onDidChangeFileDecorations.event;

    provideFileDecoration(
        uri: vscode.Uri,
        _token: vscode.CancellationToken
    ): vscode.FileDecoration | undefined {
        const uriStr = uri.toString();

        if (this.chainDirs.includes(uriStr)) {
            return {
                color: new vscode.ThemeColor('list.focusHighlightForeground'),
            };
        }

        const rank = this.rankDecorations.get(uriStr);
        if (rank !== undefined) {
            const total = rank.own + rank.incoming;
            const hasUnaudited = (this.decorations[uriStr] ?? 0) > 0;
            return {
                badge:   String(total),
                tooltip: `↑${rank.own} own  ←${rank.incoming} incoming`,
                color:   new vscode.ThemeColor(
                    hasUnaudited ? 'list.warningForeground' : 'list.deemphasizedForeground'
                ),
            };
        }

        const remaining = this.decorations[uriStr];
        if (remaining === undefined) {
            return undefined;
        }

        const color = remaining > 0
            ? new vscode.ThemeColor('list.warningForeground')
            : new vscode.ThemeColor('disabledForeground');

        return {
            badge:   remaining > 0 ? 'E' : undefined,
            tooltip: remaining > 0 ? "Contains unaudited effects" : undefined,
            color:   color
        };
    }

    public updateDecorations(uri: vscode.Uri, unaudited: number) {
        this.decorations[uri.toString()] = unaudited;
        this._onDidChangeFileDecorations.fire(uri);
    }

    public updateRankDecoration(uri: vscode.Uri, own: number, incoming: number) {
        this.rankDecorations.set(uri.toString(), { own, incoming });
        this._onDidChangeFileDecorations.fire(uri);
    }

    public clearRankDecorations() {
        const uris = [...this.rankDecorations.keys()].map(s => vscode.Uri.parse(s));
        this.rankDecorations.clear();
        this._onDidChangeFileDecorations.fire(uris);
    }

    public decorateChainRoots(uri: vscode.Uri) {
        this.chainDirs.push(uri.toString());
        this._onDidChangeFileDecorations.fire(uri);
    }
}

export const DecorationProvider = new TreeDecorationProvider();