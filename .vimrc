" =============================================================================
" VIMRC CONFIGURATION FILE
" =============================================================================

" -----------------------------------------------------------------------------
" PLUGIN MANAGER SETUP
" -----------------------------------------------------------------------------
" Using vim-plug for plugin management. For installation instructions, visit:
" https://github.com/junegunn/vim-plug

" Ensure vim-plug is installed. If not, install it.
if empty(glob('~/.vim/autoload/plug.vim'))
  silent !curl -fLo ~/.vim/autoload/plug.vim --create-dirs
        \ https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim
  autocmd VimEnter * PlugInstall --sync | source $MYVIMRC
endif

" Initialize plugin system
call plug#begin('~/.vim/plugged')

" Essential sanity settings: https://github.com/tpope/vim-sensible
Plug 'tpope/vim-sensible'

" Improved and visually appealing status line / tab line: https://github.com/vim-airline/vim-airline
Plug 'vim-airline/vim-airline'
Plug 'vim-airline/vim-airline-themes'

" Gutter visualization of version control status per line: https://github.com/airblade/vim-gitgutter
Plug 'airblade/vim-gitgutter'

" Additional useful plugins
" NerdTree for file system explorer: https://github.com/preservim/nerdtree
Plug 'preservim/nerdtree'

" Fugitive for Git integration: https://github.com/tpope/vim-fugitive
Plug 'tpope/vim-fugitive'

" Plug end - end plugin section
call plug#end()

" -----------------------------------------------------------------------------
" ESSENTIAL SETTINGS
" -----------------------------------------------------------------------------
" Set number of lines and columns for the screen
set number                 " Show line numbers
set relativenumber         " Show relative line numbers
set tabstop=4              " Number of spaces that a <Tab> in the file counts for
set shiftwidth=4           " Number of spaces to use for each step of (auto)indent
set expandtab              " Use spaces instead of tabs
set autoindent             " Copy indent from current line when starting a new line
set smartindent            " Insert indents automatically
set cursorline             " Highlight the current line
set visualbell             " Use visual bell instead of beeping when doing something wrong

" Enable mouse support
set mouse=a

" Enable syntax highlighting
syntax on

" Set color scheme (optional, choose one you like)
" colorscheme default

" -----------------------------------------------------------------------------
" PLUGIN SPECIFIC CONFIGURATIONS
" -----------------------------------------------------------------------------
" Configuration for vim-airline
" For a list of available themes, visit: https://github.com/vim-airline/vim-airline-themes
"let g:airline_theme='solarized'  " Set your preferred airline theme here
"let g:airline_solarized_bg='dark'

let g:airline#extensions#tabline#enabled = 1
let g:airline#extensions#tabline#formatter = 'unique_tail'

" Configuration for vim-gitgutter
let g:gitgutter_map_keys = 0  " Disable default mappings
nmap <Leader>hp :GitGutterPreviewHunk<CR>
nmap <Leader>hs :GitGutterStageHunk<CR>
nmap <Leader>hu :GitGutterUndoHunk<CR>

" Configuration for NERDTree
nmap <C-n> :NERDTreeToggle<CR>
let NERDTreeShowHidden=1     " Show hidden files in NERDTree

" Configuration for vim-fugitive
nmap <Leader>gs :Git<CR>     " Git status
nmap <Leader>gd :Gdiff<CR>   " Git diff

" -----------------------------------------------------------------------------
" MISCELLANEOUS SETTINGS
" -----------------------------------------------------------------------------
" Make pressing `q` safer by requiring a capital `Q`
nnoremap q <Nop>

" Leader key configuration
let mapleader=","

" Better command line completion
set wildmenu

" Enable persistent undo
set undofile
set undodir=~/.vim/undodir

" Incremental search
set incsearch

" Case insensitive searching UNLESS /C or capital in search
set ignorecase
set smartcase

" Highlight searches
set hlsearch

" -----------------------------------------------------------------------------
" CUSTOM FUNCTIONS AND MAPPINGS
" -----------------------------------------------------------------------------

" Function to create parent directories before saving the file
function! s:MkNonExDir(file, buf)
    if !isdirectory(fnamemodify(a:file, ':h'))
        call mkdir(fnamemodify(a:file, ':h'), 'p')
    endif
endfunction

" Autocmd to trigger the directory creation before writing the file
autocmd BufWritePre * call s:MkNonExDir(expand('<afile>'), bufnr(''))

" =============================================================================
" END OF .VIMRC
" =============================================================================
