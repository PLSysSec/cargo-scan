import * as vscode from 'vscode';

interface EffectResponseData {
    effect_id: string;
    effect_type: string;
    location: vscode.Location;
}

export interface EffectsResponse {
    effects: EffectResponseData[];
}

// Class to present the locations of the effects as a TreeView in VSCode's sidebar
export class LocationsProvider
    implements vscode.TreeDataProvider<vscode.TreeItem>
{
    private _onDidChangeTreeData: vscode.EventEmitter<vscode.TreeItem | undefined> = 
        new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData: vscode.Event<vscode.TreeItem | undefined> =
        this._onDidChangeTreeData.event;

    private effects: EffectResponseData[] = [];
    private groupedEffects: { [file: string]: EffectResponseData[] } = {};

    setLocations(effects: EffectResponseData[]) {
        this.effects = effects;
        this.groupedEffects = this.groupByFile(effects);
        this._onDidChangeTreeData.fire(undefined);
    }

    getTreeItem(element: LocationItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: vscode.TreeItem): Thenable<vscode.TreeItem[]> {
        if (!element) {
            // Return top-level file items
            return Promise.resolve(
                Object.keys(this.groupedEffects).map(
                    (file) => new FileItem(file)
                )
            );
        } else if (element instanceof FileItem) {
            // Return effects' locations within a file
            return Promise.resolve(
                this.groupedEffects[element.label as string].map(
                    (location) => new LocationItem(location)
                )
            );
        }
        return Promise.resolve([]);
    }

    // Group effects by their containing file
    private groupByFile(effects: EffectResponseData[]): {
        [file: string]: EffectResponseData[];
    } {
        return effects.reduce(
            (grouped, effect) => {
                const uri = vscode.Uri.parse(effect.location.uri.toString());
                const file = uri.fsPath;

                if (!grouped[file]) {
                    grouped[file] = [];
                }
                grouped[file].push(effect);
                return grouped;
            },
            {} as { [file: string]: EffectResponseData[] }
        );
    }
}

class FileItem extends vscode.TreeItem {
    constructor(public readonly label: string) {
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
    }
}

class LocationItem extends vscode.TreeItem {
    constructor(public readonly data: EffectResponseData) {
        super(
            `${data.effect_type}: ${data.location.range.start.line + 1}:${data.location.range.start.character + 1}`,
            vscode.TreeItemCollapsibleState.None
        );
        this.tooltip = data.effect_id;
        this.command = {
            command: 'effectsView.openLocation',
            title: 'Open Location',
            arguments: [this.data.location],
        };
    }
}

export async function openLocation(location: vscode.Location) {
    const uri = vscode.Uri.parse(location.uri.toString());
    const document = await vscode.workspace.openTextDocument(uri);
    const editor = await vscode.window.showTextDocument(document);
    const range = new vscode.Range(location.range.start, location.range.end);
    editor.selection = new vscode.Selection(range.start, range.end);
    editor.revealRange(range);
}
