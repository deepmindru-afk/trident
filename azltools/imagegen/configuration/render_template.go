package configuration

import (
	"crypto/sha256"
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"text/template"
)

//go:embed template/host-config.yaml.tmpl
var hostConfigTemplate string

//go:embed template/host-config-encrypted.yaml.tmpl
var hostConfigEncryptedTemplate string

// Creates Host Configuration in the specified path, by adding the user input to the template.
func RenderTridentHostConfig(configPath string, configData *TridentConfigData) error {
	configDir := filepath.Dir(configPath)
	if err := os.MkdirAll(configDir, 0755); err != nil {
		return fmt.Errorf("failed to create Host Configuration directory: %w", err)
	}

	// Create scripts directory inside config directory
	scriptsDir := filepath.Join(configDir, "scripts")
	if err := os.MkdirAll(scriptsDir, 0700); err != nil {
		return fmt.Errorf("failed to create scripts directory: %w", err)
	}

	// Write password script
	passwordScriptPath := filepath.Join(scriptsDir, "user-password.sh")
	err := passwordScript(passwordScriptPath, configData)
	if err != nil {
		return fmt.Errorf("failed to write password script: %w", err)
	}
	configData.PasswordScript = passwordScriptPath

	// Select template
	var templateContent string
	if configData.EncryptionKey != "" {
		// Generate recovery key from encryption key
		recoveryKeyPath := filepath.Join(configDir, "recovery.key")
		err := generateRecoveryKeyFromPassword(recoveryKeyPath, configData.EncryptionKey)
		if err != nil {
			return fmt.Errorf("failed to generate recovery key: %w", err)
		}
		configData.RecoveryKeyPath = recoveryKeyPath
		templateContent = hostConfigEncryptedTemplate
	} else {
		templateContent = hostConfigTemplate
	}

	// Render the config file
	tmpl, err := template.New("host-config").Parse(templateContent)
	if err != nil {
		return fmt.Errorf("failed to parse template: %w", err)
	}
	out, err := os.Create(configPath)
	if err != nil {
		return fmt.Errorf("failed to create config file: %w", err)
	}
	defer out.Close()
	return tmpl.Execute(out, configData)
}

// Creates the password script at the given path
func passwordScript(passwordScriptPath string, configData *TridentConfigData) (err error) {
	script := fmt.Sprintf("echo '%s:%s' | chpasswd\n", configData.Username, configData.Password)
	dir := filepath.Dir(passwordScriptPath)
	if err = os.MkdirAll(dir, 0700); err != nil {
		return
	}
	if err = os.WriteFile(passwordScriptPath, []byte(script), 0700); err != nil {
		return
	}
	return
}

// Generates a recovery key using a password
func generateRecoveryKeyFromPassword(keyPath, password string) error {
	// Use a simple but deterministic key derivation
	salt := []byte("trident_recovery_salt_v1")
	iterations := 100000
	keyLength := 64

	// Simple PBKDF2-like implementation using repeated hashing
	key := make([]byte, keyLength)
	current := sha256.Sum256(append([]byte(password), salt...))

	// Multiple rounds for key stretching
	for i := 0; i < iterations; i++ {
		h := sha256.New()
		h.Write(current[:])
		h.Write([]byte(password))
		h.Write(salt)
		current = [32]byte(h.Sum(nil))
	}

	// Expand to 64 bytes
	for i := 0; i < keyLength; i += 32 {
		copy(key[i:], current[:])
		if i+32 < keyLength {
			// Generate next 32 bytes
			h := sha256.New()
			h.Write(current[:])
			h.Write([]byte{byte(i / 32)})
			current = [32]byte(h.Sum(nil))
		}
	}

	// Write key to file with proper permissions
	if err := os.WriteFile(keyPath, key, 0400); err != nil {
		return fmt.Errorf("failed to write recovery key: %w", err)
	}

	return nil
}
