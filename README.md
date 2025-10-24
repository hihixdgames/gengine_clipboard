# gengine_clipboard: a flexible multiplaform clipboard solution

gengine_clipboard allows users to write their own logic for choosing and processing clipboard data, while abstracting away platform-specific behavior.

This library is being developed for an internal game engine at [hihi XD](https://hihixd.com/), and features are implemented as they are needed by games.

## Supported Features\Platforms
|Feature   |Linux     |Windows   |Wasm (Web)|macOS     |
|:--------:|:--------:|:--------:|:--------:|:--------:|
|Read Data |✅        |✅        |⚠️        |❌        | 
|Write Data|❌        | ❌       | ❌       |❌        | 

✅ = Supported  
❌ = Not Supported  
⚠️ = You can't read clipboard content whenever. Instead you need to wait for events. 

## Development todo
- Proper error handling and miminize potential program crashes
- writing data to the clipboard
