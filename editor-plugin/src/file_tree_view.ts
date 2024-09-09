import * as vscode from 'vscode';
import * as path from 'path';

export interface EffectResponseData {
    caller: string;
    callee: string;
    effect_type: string;
    location: vscode.Location;
    crate_name: string;
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
        this.groupByFile(effects);
        this.sortGroupedEffects();
        this._onDidChangeTreeData.fire(undefined);    
    }

    getTreeItem(element: LocationItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: vscode.TreeItem): Thenable<vscode.TreeItem[]> {
        if (!element) {
            const crates = this.getCrateItems();
            if (crates.length <= 1) {
                return Promise.resolve(crates);
            }

            const root = new DirectoryItem(vscode.Uri.file("root"), "Root Crate");
            const deps = new DirectoryItem(vscode.Uri.file("dependencies"), "Dependencies");
            const wsName = vscode.workspace.workspaceFolders?.map((folder) => folder.name)[0];
            crates.forEach(crate => crate.label === wsName ? root.addChild(crate) : deps.addChild(crate));

            return Promise.resolve([root, deps]);
        } else if (element instanceof DirectoryItem) {
            return Promise.resolve(element.getChildren());
        } else if (element instanceof FileItem) {
            return Promise.resolve(element.getChildren());
        }
        return Promise.resolve([]);
    }

    // Group effects by their containing file
    private groupByFile(effects: EffectResponseData[]) {
        for (const effect of effects) {
            const uri =  effect.location.uri;
            const file = uri.fsPath;

            if (!this.groupedEffects[file]) {
                this.groupedEffects[file] = [];
            }
            
            if (!this.groupedEffects[file].some(e => e.location.range.isEqual(effect.location.range) 
                && e.callee == effect.callee)) {
                this.groupedEffects[file].push(effect);
            }
        }
    }

    private getCrateItems(): vscode.TreeItem[] {
        const crates: vscode.TreeItem[] = [];
    
        // Loop through the effects and build directory hierarchy
        for (const file in this.groupedEffects) {
            const effects = this.groupedEffects[file];
            const crateName = effects[0].crate_name; 
            const relativePath = this.getRelativeFilePath(file, crateName);
            this.buildDirectories(relativePath, effects, crates);
        }

        // Sort alphabetically by crate name
        return crates.sort((a, b) => {
            const aLabel = typeof a.label === 'string' ? a.label : a.label?.label ?? '';
            const bLabel = typeof b.label === 'string' ? b.label : b.label?.label ?? '';
            return aLabel.localeCompare(bLabel);
        });
    }

    // Get the file path relative to the crate root
    private getRelativeFilePath(filePath: string, crateName: string): string {
        const segments = filePath.split(path.sep);    
        const name = crateName.replace(/_/g, '-');
        const idx = segments.findIndex((segment) => {
            return segment.replace(/_/g, '-').startsWith(name);
        });
    
        return idx !== -1 ? path.join(...segments.slice(idx)) : filePath;
    }
    

    private buildDirectories(
        filePath: string,
        effects: EffectResponseData[],
        parentItems: vscode.TreeItem[]
    ) {          
        const segments = filePath.split(path.sep);
        segments.forEach((segment, index) => {
            if (index === segments.length - 1) {
                // The last segment is the filename
                const fileItem = new FileItem(vscode.Uri.file(filePath), effects);
                parentItems.push(fileItem);
            } else {
                let dirItem = parentItems.find(
                    (item) => item instanceof DirectoryItem && item.label === segment
                ) as DirectoryItem;
    
                if (!dirItem) {
                    dirItem = new DirectoryItem(vscode.Uri.file(segment), segment);
                    parentItems.push(dirItem);
                }
    
                parentItems = dirItem.getChildren();
            }
        });
    }
    
    getGroupedEffects(): { [file: string]: EffectResponseData[] } {
        return this.groupedEffects;
    }

    register(context: vscode.ExtensionContext) {
        const tree = vscode.window.createTreeView('effectsView', { treeDataProvider: this });    
        context.subscriptions.push(tree);
    }

    clear() {
        this.groupedEffects = {};
        this._onDidChangeTreeData.fire(undefined); 
    }

    private sortGroupedEffects() {
        for (const file in this.groupedEffects) {
            this.groupedEffects[file].sort((a, b) => a.location.range.start.compareTo(b.location.range.start));
        }
    }
}

class DirectoryItem extends vscode.TreeItem {
    private children: vscode.TreeItem[] = [];
    constructor(
        public readonly resourceUri: vscode.Uri,
        public readonly label: string
    ) {
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
    }

    getChildren(): vscode.TreeItem[] {
        return this.children;
    }

    addChild(item: vscode.TreeItem) {
        this.children.push(item);
    }
}

class FileItem extends vscode.TreeItem {
    constructor(
        public readonly resourceUri: vscode.Uri,
        public readonly effects: EffectResponseData[]
    ) {
        const label = `${path.basename(resourceUri.fsPath)}`;
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
    }

    getChildren(): vscode.TreeItem[] {
        // Return the effects' locations within the file
        return this.effects.map(
            (effect) => new LocationItem(effect)
        );
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
