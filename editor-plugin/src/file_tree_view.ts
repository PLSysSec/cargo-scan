import * as vscode from 'vscode';
import { window } from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';

interface EffectResponseData {
    caller: string;
    callee: string;
    effect_type: string;
    location: vscode.Location;
}

export interface EffectsResponse {
    effects: EffectResponseData[];
}

interface AuditNotification {
    safety_annotation: string;
    effect: EffectResponseData;
}

// Class to present the locations of the effects as a TreeView in VSCode's sidebar
export class LocationsProvider
    implements vscode.TreeDataProvider<vscode.TreeItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<vscode.TreeItem | undefined> =
        new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData: vscode.Event<vscode.TreeItem | undefined> =
        this._onDidChangeTreeData.event;

    private groupedEffects: { [file: string]: EffectResponseData[] } = {};

    constructor(private client: LanguageClient) {}

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

    private remove_item(item: LocationItem) {
        const uri = vscode.Uri.parse(item.data.location.uri.toString());
        const file = uri.fsPath;
        let effects = this.groupedEffects[file];

        // Remove effect item from the list
        if (effects) {
            this.groupedEffects[file] = effects.filter(eff => eff !== item.data);
        }
        // If there no remaining effects for `file`,
        // delete it from the TreeView as well
        if (this.groupedEffects[file].length === 0) {
            delete this.groupedEffects[file];
        }
        // Update TreeView
        this._onDidChangeTreeData.fire(undefined);
    }

    register(context: vscode.ExtensionContext) {
        const tree = vscode.window.createTreeView('effectsView', { treeDataProvider: this });

        tree.onDidChangeSelection(async (e) => {
            const annotate_effects = context.globalState.get('annotateEffects', false);
            const selectedItem = e.selection[0];

            if (selectedItem instanceof LocationItem) {
                if (annotate_effects) {
                    const selection = await vscode.window.showQuickPick(['Safe', 'Unsafe', 'Caller-Checked'], {
                        placeHolder: 'Choose a safety annotation for this effect instance'
                    });
                    if (selection) {
                        vscode.window.showInformationMessage(`You marked effect "${selectedItem.tooltip}" as ${selection}.`);
                        this.remove_item(selectedItem);

                        // Notify server about the received safety annotation from the user
                        const params: AuditNotification = { safety_annotation: selection, effect: selectedItem.data };
                        this.client.sendNotification('cargo-scan.set_annotation', params);
                    }
                } else {
                    // Preview effects
                    let location = selectedItem.data.location;
                    vscode.commands.executeCommand('vscode.open', location.uri, { selection: location.range });
                }
            }
        })

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
