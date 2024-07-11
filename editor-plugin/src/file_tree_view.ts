import * as vscode from 'vscode';

export interface EffectResponseData {
    caller: string;
    callee: string;
    effect_type: string;
    location: vscode.Location;
}

export interface EffectsResponse {
    effects: EffectResponseData[];
}

// Class to present the locations of the effects as a TreeView in VSCode's sidebar
export class LocationsProvider
    implements vscode.TreeDataProvider<vscode.TreeItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<vscode.TreeItem | undefined> =
        new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData: vscode.Event<vscode.TreeItem | undefined> =
        this._onDidChangeTreeData.event;

    private groupedEffects: { [file: string]: EffectResponseData[] } = {};

    setLocations(effects: EffectResponseData[]) {
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

    getGroupedEffects(): { [file: string]: EffectResponseData[] } {
        return this.groupedEffects;
    }

    register(context: vscode.ExtensionContext) {
        const tree = vscode.window.createTreeView('effectsView', { treeDataProvider: this });    
        context.subscriptions.push(tree);
    }
}

class FileItem extends vscode.TreeItem {
    constructor(public readonly label: string) {
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
    }
}

class LocationItem extends vscode.TreeItem {
    constructor(public readonly data: EffectResponseData) {
        let start = data.location.range.start;
        super(
            `${data.effect_type}: ${start.line + 1}:${start.character + 1}`,
            vscode.TreeItemCollapsibleState.None
        );

        this.tooltip = data.callee;
        this.command = {
            command: 'vscode.open',
            title: 'Open Location',
            arguments: [data.location.uri, { selection: data.location.range }]
        };
    }
}
