#!/usr/bin/env python3
"""
Script to fix the systemic issue where request headers are being reused as response headers.
This causes content-length mismatches and hyper panics.
"""

import os
import re
import sys

def fix_file(filepath):
    """Fix header reuse issues in a single file."""
    print(f"Processing {filepath}...")
    
    with open(filepath, 'r') as f:
        content = f.read()
    
    original_content = content
    changes_made = 0
    
    # Pattern to match function signatures that take mut headers: HeaderMap
    # and replace them to create fresh headers inside the function
    pattern = r'(pub async fn \w+\([^)]*?)\s*mut headers: HeaderMap,([^)]*?\)) -> Result<\([^)]*HeaderMap[^)]*\)> \{\s*(super::add_rate_limit_headers\(&mut headers\)|add_rate_limit_headers\(&mut headers\));'
    
    def replacement(match):
        func_start = match.group(1)
        func_end = match.group(2)
        rate_limit_call = match.group(3)
        
        # Remove mut headers: HeaderMap from parameters
        new_func = func_start.rstrip(' ').rstrip(',') + func_end + ') -> Result<(StatusCode, HeaderMap, Json<' + 'T' + '>)> {\n    let mut headers = HeaderMap::new();\n    ' + rate_limit_call + ';'
        return new_func
    
    # More precise regex for different patterns
    patterns = [
        # Pattern 1: mut headers: HeaderMap in middle of params
        (r'(\s+mut headers: HeaderMap,)(\s+[^)]+\)) -> Result<\(([^)]+)\)> \{\s*((?:super::)?add_rate_limit_headers\(&mut headers\));',
         r'\2) -> Result<(\3)> {\n    let mut headers = HeaderMap::new();\n    \4;'),
        
        # Pattern 2: mut headers: HeaderMap at end of params  
        (r'(\s+auth_session: AuthSession,)\s*mut headers: HeaderMap,(\s*\)) -> Result<\(([^)]+)\)> \{\s*((?:super::)?add_rate_limit_headers\(&mut headers\));',
         r'\1\2) -> Result<(\3)> {\n    let mut headers = HeaderMap::new();\n    \4;'),
         
        # Pattern 3: only mut headers: HeaderMap param
        (r'(\s+mut auth_session: AuthSession,)\s*mut headers: HeaderMap,(\s*\)) -> Result<\(([^)]+)\)> \{\s*((?:super::)?add_rate_limit_headers\(&mut headers\));',
         r'\1\2) -> Result<(\3)> {\n    let mut headers = HeaderMap::new();\n    \4;'),
    ]
    
    for pattern, replacement in patterns:
        new_content = re.sub(pattern, replacement, content, flags=re.MULTILINE)
        if new_content != content:
            changes_made += len(re.findall(pattern, content))
            content = new_content
    
    # More general approach - find and replace each occurrence
    lines = content.split('\n')
    new_lines = []
    i = 0
    
    while i < len(lines):
        line = lines[i]
        
        # Look for function signatures with mut headers: HeaderMap
        if 'mut headers: HeaderMap' in line and 'pub async fn' in line:
            # Find the complete function signature
            func_lines = [line]
            j = i + 1
            while j < len(lines) and not lines[j].strip().startswith(') -> Result<'):
                func_lines.append(lines[j])
                j += 1
            if j < len(lines):
                func_lines.append(lines[j])  # The ') -> Result<...' line
                
            # Join and fix the function signature
            func_signature = '\n'.join(func_lines)
            
            # Remove 'mut headers: HeaderMap,' from parameters
            fixed_signature = re.sub(r',?\s*mut headers: HeaderMap,?', '', func_signature)
            fixed_signature = re.sub(r',(\s*\))', r'\1', fixed_signature)  # Remove trailing comma
            
            new_lines.extend(fixed_signature.split('\n'))
            
            # Look for the opening brace and add fresh headers
            k = j + 1
            while k < len(lines) and '{' not in lines[k]:
                new_lines.append(lines[k])
                k += 1
            
            if k < len(lines):
                brace_line = lines[k]
                new_lines.append(brace_line)
                
                # Look for the rate limit call and add fresh headers before it
                m = k + 1
                while m < len(lines) and 'add_rate_limit_headers' not in lines[m]:
                    new_lines.append(lines[m])
                    m += 1
                
                if m < len(lines):
                    # Add fresh headers before rate limit call
                    indent = '    '  # Standard 4-space indent
                    new_lines.append(f'{indent}let mut headers = HeaderMap::new();')
                    new_lines.append(lines[m])  # The rate limit call
                    i = m
                else:
                    i = k
            else:
                i = j
            changes_made += 1
        else:
            new_lines.append(line)
            i += 1
    
    if changes_made > 0:
        content = '\n'.join(new_lines)
        
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"  Fixed {changes_made} function(s)")
        return True
    else:
        print("  No changes needed")
        return False

def main():
    """Fix all Rust files in the api directory."""
    api_dir = 'src/api'
    if not os.path.exists(api_dir):
        print(f"Error: {api_dir} directory not found")
        sys.exit(1)
    
    rust_files = []
    for filename in os.listdir(api_dir):
        if filename.endswith('.rs'):
            rust_files.append(os.path.join(api_dir, filename))
    
    total_fixed = 0
    for filepath in sorted(rust_files):
        if fix_file(filepath):
            total_fixed += 1
    
    print(f"\nSummary: Fixed {total_fixed} file(s)")

if __name__ == '__main__':
    main()